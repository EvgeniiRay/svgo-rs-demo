//! Port of `plugins/minifyStyles.js` (CSSO behaviour approximated in `css.rs`).

use crate::css::{self, AtruleBlock, CssNode};
use crate::dom::{collect_element_uids, detach_by_uid, element_by_uid, element_by_uid_mut, Node};
use crate::json::Json;
use crate::tools::has_scripts;
use std::collections::HashSet;

#[derive(Clone, Default)]
struct Usage {
    enabled: bool,
    tags: HashSet<String>,
    ids: HashSet<String>,
    classes: HashSet<String>,
    enable_tags: bool,
    enable_ids: bool,
    enable_classes: bool,
}

fn selector_requirements(selector: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
    // (tags, ids, classes)
    let mut tags = Vec::new();
    let mut ids = Vec::new();
    let mut classes = Vec::new();
    if let Some(sel) = crate::selector::parse_selector(selector) {
        for compound in &sel.compounds {
            for simple in &compound.0 {
                match simple {
                    crate::selector::Simple::Tag(t) => tags.push(t.clone()),
                    crate::selector::Simple::Id(i) => ids.push(i.clone()),
                    crate::selector::Simple::Class(c) => classes.push(c.clone()),
                    _ => {}
                }
            }
        }
    }
    (tags, ids, classes)
}

fn selector_used(selector: &str, usage: &Usage) -> bool {
    if !usage.enabled {
        return true;
    }
    let (tags, ids, classes) = selector_requirements(selector);
    if usage.enable_classes && classes.iter().any(|c| !usage.classes.contains(c)) {
        return false;
    }
    if usage.enable_ids && ids.iter().any(|i| !usage.ids.contains(i)) {
        return false;
    }
    if usage.enable_tags && tags.iter().any(|t| !usage.tags.contains(t)) {
        return false;
    }
    true
}

fn is_keyframes(name: &str) -> bool {
    let base = name.rsplit('-').next().unwrap_or(name);
    base == "keyframes" || name == "keyframes" || name.ends_with("keyframes")
}

fn convert_keyframe_selectors(selectors: &mut [String]) {
    for s in selectors.iter_mut() {
        *s = s
            .split(',')
            .map(|part| {
                let p = part.trim();
                match p {
                    "100%" => "to".to_string(),
                    "from" => "0%".to_string(),
                    _ => p.to_string(),
                }
            })
            .collect::<Vec<_>>()
            .join(",");
    }
}

fn decls_equal(a: &[crate::css::Decl], b: &[crate::css::Decl]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b.iter()).all(|(x, y)| {
            x.property.to_lowercase() == y.property.to_lowercase()
                && x.value == y.value
                && x.important == y.important
        })
}

// Merge rules the way csso restructure does for the safe cases:
// adjacent rules with identical selectors (append declarations) or
// identical declarations (append selectors). Inside @keyframes the
// same merges are also applied non-adjacently, since keyframe
// selectors for distinct offsets never cascade against each other.
fn merge_rules(nodes: &mut Vec<CssNode>, in_keyframes: bool) {
    let mut out: Vec<CssNode> = Vec::new();
    for node in nodes.drain(..) {
        let CssNode::Rule { mut selectors, decls } = node else {
            out.push(node);
            continue;
        };
        if in_keyframes {
            convert_keyframe_selectors(&mut selectors);
        }
        let decls = css::minify_decls(&decls);
        if decls.is_empty() {
            continue;
        }
        // same-selector merge
        let same_sel = out.iter().rposition(|n| match n {
            CssNode::Rule { selectors: s, .. } => {
                s.iter().map(|x| css::gen_selector(x)).collect::<Vec<_>>()
                    == selectors.iter().map(|x| css::gen_selector(x)).collect::<Vec<_>>()
            }
            _ => false,
        });
        if let Some(pos) = same_sel {
            if in_keyframes || pos + 1 == out.len() {
                if let CssNode::Rule { decls: prev, .. } = &mut out[pos] {
                    let mut merged = prev.clone();
                    merged.extend(decls.clone());
                    *prev = css::minify_decls(&merged);
                }
                continue;
            }
        }
        // equal-declarations merge
        let same_decl = out.iter().rposition(|n| match n {
            CssNode::Rule { decls: d, .. } => decls_equal(d, &decls),
            _ => false,
        });
        if let Some(pos) = same_decl {
            if in_keyframes || pos + 1 == out.len() {
                if let CssNode::Rule { selectors: prev, .. } = &mut out[pos] {
                    for s in selectors {
                        if !prev.iter().any(|x| css::gen_selector(x) == css::gen_selector(&s)) {
                            prev.push(s);
                        }
                    }
                }
                continue;
            }
        }
        out.push(CssNode::Rule { selectors, decls });
    }
    *nodes = out;
}

fn minify_nodes(nodes: &mut Vec<CssNode>, usage: &Usage) {
    minify_nodes_inner(nodes, usage, false);
}

fn minify_nodes_inner(nodes: &mut Vec<CssNode>, usage: &Usage, in_keyframes: bool) {
    // csso clean: a stray @import preceded by anything other than
    // @charset/@import is removed
    let mut seen_non_import = false;
    nodes.retain(|n| match n {
        CssNode::Atrule { name, block: None, .. } if name.to_lowercase() == "import" => {
            !seen_non_import
        }
        CssNode::Atrule { name, .. } if matches!(name.to_lowercase().as_str(), "charset" | "import") => {
            true
        }
        _ => {
            seen_non_import = true;
            true
        }
    });
    let mut retained: Vec<CssNode> = Vec::new();
    for node in nodes.drain(..) {
        match node {
            CssNode::Rule { selectors, decls } => {
                let kept: Vec<String> =
                    selectors.into_iter().filter(|s| selector_used(s, usage)).collect();
                if kept.is_empty() {
                    continue;
                }
                let decls = css::minify_decls(&decls);
                if decls.is_empty() {
                    continue;
                }
                retained.push(CssNode::Rule { selectors: kept, decls });
            }
            CssNode::Atrule { name, prelude, block } => {
                let kf = is_keyframes(&name);
                let block = match block {
                    Some(AtruleBlock::Rules(mut inner)) => {
                        if kf {
                            // keyframe rules are exempt from usage filtering in csso
                            let mut no_usage = usage.clone();
                            no_usage.enabled = false;
                            minify_nodes_inner(&mut inner, &no_usage, true);
                        } else {
                            minify_nodes_inner(&mut inner, usage, false);
                        }
                        if inner.is_empty() {
                            continue;
                        }
                        Some(AtruleBlock::Rules(inner))
                    }
                    Some(AtruleBlock::Decls(decls)) => {
                        let decls = css::minify_decls(&decls);
                        if decls.is_empty() {
                            continue;
                        }
                        Some(AtruleBlock::Decls(decls))
                    }
                    None => None,
                };
                retained.push(CssNode::Atrule { name, prelude, block });
            }
        }
    }
    *nodes = retained;
    merge_rules(nodes, in_keyframes);
}

fn relocate_keyframes(nodes: &mut Vec<CssNode>) {
    let mut keyframes: Vec<CssNode> = Vec::new();
    let mut rest: Vec<CssNode> = Vec::new();
    for node in nodes.drain(..) {
        match &node {
            CssNode::Atrule { name, .. } if is_keyframes(name) => keyframes.push(node),
            _ => rest.push(node),
        }
    }
    if keyframes.is_empty() {
        *nodes = rest;
        return;
    }
    // insert before the first node that is not @charset/@import
    let pos = rest
        .iter()
        .position(|n| !matches!(n, CssNode::Atrule { name, .. } if matches!(name.to_lowercase().as_str(), "charset" | "import")))
        .unwrap_or(rest.len());
    let mut out: Vec<CssNode> = Vec::new();
    out.extend(rest.drain(..pos));
    out.extend(keyframes);
    out.extend(rest);
    *nodes = out;
}

fn minify_css(css_text: &str, usage: &Usage) -> String {
    let mut nodes = css::parse_stylesheet(css_text);
    minify_nodes(&mut nodes, usage);
    relocate_keyframes(&mut nodes);
    css::generate(&nodes, true)
}

pub fn apply(root: &mut Vec<Node>, params: &Json) {
    // usage param
    let (mut enable_tags, mut enable_ids, mut enable_classes, mut force) = (true, true, true, false);
    match params.get("usage") {
        Some(Json::Bool(b)) => {
            enable_tags = *b;
            enable_ids = *b;
            enable_classes = *b;
        }
        Some(Json::Obj(_)) => {
            let u = params.get("usage").unwrap();
            if let Some(v) = u.get("tags") {
                enable_tags = v.as_bool(true);
            }
            if let Some(v) = u.get("ids") {
                enable_ids = v.as_bool(true);
            }
            if let Some(v) = u.get("classes") {
                enable_classes = v.as_bool(true);
            }
            if let Some(v) = u.get("force") {
                force = v.as_bool(false);
            }
        }
        _ => {}
    }

    let mut style_uids: Vec<u64> = Vec::new();
    let mut style_attr_uids: Vec<u64> = Vec::new();
    let mut tags: HashSet<String> = HashSet::new();
    let mut ids: HashSet<String> = HashSet::new();
    let mut classes: HashSet<String> = HashSet::new();
    let mut deoptimized = false;

    let mut all_uids = Vec::new();
    collect_element_uids(root, &mut all_uids);
    for uid in &all_uids {
        let Some(e) = element_by_uid(root, *uid) else { continue };
        if has_scripts(e) {
            deoptimized = true;
        }
        tags.insert(e.name.clone());
        if let Some(id) = e.get("id") {
            ids.insert(id.to_string());
        }
        if let Some(cls) = e.get("class") {
            for c in cls.split_whitespace() {
                classes.insert(c.to_string());
            }
        }
        if e.name == "style" && !e.children.is_empty() {
            style_uids.push(*uid);
        } else if e.get("style").is_some() {
            style_attr_uids.push(*uid);
        }
    }

    let usage_enabled = !deoptimized || force;
    let usage = Usage {
        enabled: usage_enabled,
        tags,
        ids,
        classes,
        enable_tags,
        enable_ids,
        enable_classes,
    };

    for uid in style_uids {
        let (css_text, first_is_text_or_cdata) = {
            let Some(e) = element_by_uid(root, uid) else { continue };
            match e.children.first() {
                Some(Node::Text(v)) => (v.clone(), true),
                Some(Node::Cdata(v)) => (v.clone(), true),
                _ => (String::new(), false),
            }
        };
        if !first_is_text_or_cdata {
            continue;
        }
        let minified = minify_css(&css_text, &usage);
        if minified.is_empty() {
            detach_by_uid(root, uid);
            continue;
        }
        let use_cdata = css_text.contains('>') || css_text.contains('<');
        if let Some(e) = element_by_uid_mut(root, uid) {
            e.children[0] = if use_cdata {
                Node::Cdata(minified)
            } else {
                Node::Text(minified)
            };
        }
    }
    for uid in style_attr_uids {
        if let Some(e) = element_by_uid_mut(root, uid) {
            if let Some(style) = e.get("style").map(|s| s.to_string()) {
                let minified = css::minify_block(&style);
                e.set("style", &minified);
            }
        }
    }
}
