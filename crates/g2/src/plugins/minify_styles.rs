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

fn minify_nodes(nodes: &mut Vec<CssNode>, usage: &Usage) {
    let mut retained: Vec<CssNode> = Vec::new();
    for node in nodes.drain(..) {
        match node {
            CssNode::Rule { selectors, decls } => {
                let kept: Vec<String> =
                    selectors.into_iter().filter(|s| selector_used(s, usage)).collect();
                if kept.is_empty() {
                    continue;
                }
                retained.push(CssNode::Rule { selectors: kept, decls: css::minify_decls(&decls) });
            }
            CssNode::Atrule { name, prelude, block } => {
                let block = match block {
                    Some(AtruleBlock::Rules(mut inner)) => {
                        minify_nodes(&mut inner, usage);
                        Some(AtruleBlock::Rules(inner))
                    }
                    Some(AtruleBlock::Decls(decls)) => {
                        Some(AtruleBlock::Decls(css::minify_decls(&decls)))
                    }
                    None => None,
                };
                retained.push(CssNode::Atrule { name, prelude, block });
            }
        }
    }
    *nodes = retained;
}

fn minify_css(css_text: &str, usage: &Usage) -> String {
    let mut nodes = css::parse_stylesheet(css_text);
    minify_nodes(&mut nodes, usage);
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
