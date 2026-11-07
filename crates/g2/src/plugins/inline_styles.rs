//! Port of `plugins/inlineStyles.js`.

use crate::collections::{is_preserved_pseudo, is_presentation};
use crate::css::{self, AtruleBlock, CssNode, Decl};
use crate::dom::{detach_by_uid, element_by_uid, element_by_uid_mut, Node};
use crate::json::Json;
use crate::selector::{
    includes_attr_selector, parse_selector, query_selector_all, specificity, Simple,
};

struct SelectorRef {
    style_idx: usize,
    rule_path: Vec<usize>,
    selector_text: String, // after pseudo stripping
    decls: Vec<Decl>,
    matched: Vec<u64>,
}

struct StyleInfo {
    uid: u64,
    nodes: Vec<CssNode>,
}

fn with_rule_mut<R>(
    nodes: &mut [CssNode],
    path: &[usize],
    f: impl FnOnce(&mut Vec<String>, &mut Vec<Decl>) -> R,
) -> Option<R> {
    if path.is_empty() {
        return None;
    }
    match &mut nodes[path[0]] {
        CssNode::Rule { selectors, decls } if path.len() == 1 => Some(f(selectors, decls)),
        CssNode::Atrule { block: Some(AtruleBlock::Rules(inner)), .. } if path.len() > 1 => {
            with_rule_mut(inner, &path[1..], f)
        }
        _ => None,
    }
}

fn collect_selectors_from_nodes(
    nodes: &mut [CssNode],
    path_prefix: &mut Vec<usize>,
    atrule: Option<(String, String)>,
    style_idx: usize,
    use_mqs: &[String],
    use_pseudos: &[String],
    out: &mut Vec<SelectorRef>,
) {
    for (i, node) in nodes.iter_mut().enumerate() {
        path_prefix.push(i);
        match node {
            CssNode::Rule { selectors, decls } => {
                let media_query = match &atrule {
                    None => String::new(),
                    Some((name, prelude)) => {
                        if prelude.is_empty() {
                            name.clone()
                        } else {
                            format!("{} {}", name, css::gen_prelude(prelude))
                        }
                    }
                };
                if use_mqs.iter().any(|m| m == &media_query) {
                    for sel in selectors.iter_mut() {
                        let (stripped, pseudo_text) = strip_pseudos(sel, use_pseudos);
                        let _ = pseudo_text;
                        // JS mutates the css-tree selector when pseudos are
                        // stripped; mirror that in the stored stylesheet.
                        *sel = stripped.clone();
                        out.push(SelectorRef {
                            style_idx,
                            rule_path: path_prefix.clone(),
                            selector_text: stripped,
                            decls: decls.clone(),
                            matched: Vec::new(),
                        });
                    }
                }
            }
            CssNode::Atrule { name, prelude, block } => {
                let name = name.clone();
                let prelude = prelude.clone();
                if let Some(AtruleBlock::Rules(inner)) = block {
                    collect_selectors_from_nodes(
                        inner,
                        path_prefix,
                        Some((name, prelude)),
                        style_idx,
                        use_mqs,
                        use_pseudos,
                        out,
                    );
                }
            }
        }
        path_prefix.pop();
    }
}

/// Returns (selector after possible pseudo removal, concatenated pseudo text).
fn strip_pseudos(selector: &str, use_pseudos: &[String]) -> (String, String) {
    let Some(parsed) = parse_selector(selector) else {
        return (selector.to_string(), String::new());
    };
    let mut pseudo_texts: Vec<String> = Vec::new();
    for compound in &parsed.compounds {
        for simple in &compound.0 {
            if let Simple::Pseudo { name, raw, .. } = simple {
                if !is_preserved_pseudo(name) {
                    pseudo_texts.push(raw.clone());
                }
            }
        }
    }
    let pseudo_concat = pseudo_texts.join("");
    if pseudo_concat.is_empty() || !use_pseudos.iter().any(|p| p == &pseudo_concat) {
        return (selector.to_string(), pseudo_concat);
    }
    // Remove those pseudo raw strings (first occurrence each, in order).
    // Rebuild from parsed structure is safer: drop matching pseudo simples.
    let mut out_compounds: Vec<String> = Vec::new();
    // We rebuild textually by removing raw occurrences sequentially.
    let mut result = selector.to_string();
    for raw in &pseudo_texts {
        if let Some(pos) = result.find(raw.as_str()) {
            result.replace_range(pos..pos + raw.len(), "");
        }
    }
    let _ = &mut out_compounds;
    (result, pseudo_concat)
}

fn prune_empty_rules(nodes: &mut Vec<CssNode>) {
    let mut retained = Vec::new();
    for node in nodes.drain(..) {
        match node {
            CssNode::Rule { selectors, .. } if selectors.is_empty() => {}
            CssNode::Atrule { name, prelude, block } => {
                let block = match block {
                    Some(AtruleBlock::Rules(mut inner)) => {
                        prune_empty_rules(&mut inner);
                        Some(AtruleBlock::Rules(inner))
                    }
                    other => other,
                };
                retained.push(CssNode::Atrule { name, prelude, block });
            }
            other => retained.push(other),
        }
    }
    *nodes = retained;
}

pub fn apply(root: &mut Vec<Node>, params: &Json) {
    let only_matched_once = params.get("onlyMatchedOnce").map(|v| v.as_bool(true)).unwrap_or(true);
    let remove_matched = params
        .get("removeMatchedSelectors")
        .map(|v| v.as_bool(true))
        .unwrap_or(true);
    let use_mqs: Vec<String> = params
        .get("useMqs")
        .map(|v| v.as_str_list())
        .unwrap_or_else(|| vec![String::new(), "screen".to_string()]);
    let use_pseudos: Vec<String> =
        params.get("usePseudos").map(|v| v.as_str_list()).unwrap_or_else(|| vec![String::new()]);

    // collect style elements (skip foreignObject subtrees)
    let mut style_uids: Vec<u64> = Vec::new();
    fn walk(nodes: &[Node], out: &mut Vec<u64>) {
        for n in nodes {
            if let Node::Element(e) = n {
                if e.name == "foreignObject" {
                    continue;
                }
                if e.name == "style" && !e.children.is_empty() {
                    out.push(e.uid);
                }
                walk(&e.children, out);
            }
        }
    }
    walk(root, &mut style_uids);

    let mut styles: Vec<StyleInfo> = Vec::new();
    let mut selectors: Vec<SelectorRef> = Vec::new();
    for uid in style_uids {
        let (valid, css_text) = {
            let Some(e) = element_by_uid(root, uid) else { continue };
            let valid = match e.get("type") {
                None => true,
                Some(t) => t.is_empty() || t == "text/css",
            };
            (valid, e.text_content())
        };
        if !valid {
            continue;
        }
        let nodes = css::parse_stylesheet(&css_text);
        let style_idx = styles.len();
        styles.push(StyleInfo { uid, nodes });
        // collect selectors from the stored copy
        collect_selectors_from_nodes(
            &mut styles[style_idx].nodes,
            &mut Vec::new(),
            None,
            style_idx,
            &use_mqs,
            &use_pseudos,
            &mut selectors,
        );
    }
    if styles.is_empty() {
        return;
    }

    // sort by specificity ascending (stable), then reverse
    let mut sorted_indices: Vec<usize> = (0..selectors.len()).collect();
    sorted_indices.sort_by(|&a, &b| {
        specificity(&selectors[a].selector_text).cmp(&specificity(&selectors[b].selector_text))
    });
    sorted_indices.reverse();

    // For quick "any selector includes attr" checks we need all selector texts.
    let all_selector_texts: Vec<String> =
        selectors.iter().map(|s| s.selector_text.clone()).collect();

    for &sel_idx in &sorted_indices {
        let selector_text_gen = {
            let s = &selectors[sel_idx];
            css::gen_selector(&s.selector_text)
        };
        let matched = query_selector_all(root, &selector_text_gen);
        if matched.is_empty() {
            continue;
        }
        selectors[sel_idx].matched = matched.clone();
        if only_matched_once && matched.len() > 1 {
            continue;
        }
        let decls = selectors[sel_idx].decls.clone();
        for &uid in &matched {
            // presentation-attr removal + style merge, mutating one element
            let Some(elem) = element_by_uid_mut(root, uid) else { continue };
            let mut style_decls = css::parse_decl_list(elem.get("style").unwrap_or(""));
            let mut insert_idx = 0usize;
            for decl in &decls {
                if is_presentation(&decl.property)
                    && !all_selector_texts
                        .iter()
                        .any(|t| includes_attr_selector(t, &decl.property, None, false))
                {
                    elem.remove_attr(&decl.property);
                }
                let prop_lower = decl.property.to_lowercase();
                let existing = style_decls
                    .iter()
                    .position(|d| d.property.to_lowercase() == prop_lower);
                match existing {
                    None => {
                        let at = insert_idx.min(style_decls.len());
                        style_decls.insert(at, decl.clone());
                        insert_idx = at + 1;
                    }
                    Some(pos) => {
                        if !style_decls[pos].important && decl.important {
                            style_decls[pos] = decl.clone();
                        }
                    }
                }
            }
            let new_styles = css::gen_decls(&style_decls, false);
            if !new_styles.is_empty() {
                elem.set("style", &new_styles);
            }
        }
        if remove_matched {
            let style_idx = selectors[sel_idx].style_idx;
            let rule_path = selectors[sel_idx].rule_path.clone();
            let target = selectors[sel_idx].selector_text.clone();
            if let Some(style) = styles.get_mut(style_idx) {
                with_rule_mut(&mut style.nodes, &rule_path, |sels, _decls| {
                    if let Some(pos) = sels.iter().position(|s| s == &target) {
                        sels.remove(pos);
                    }
                });
            }
        }
    }

    if !remove_matched {
        return;
    }

    // clean up matched class + ID attribute values
    for &sel_idx in &sorted_indices {
        let matched = selectors[sel_idx].matched.clone();
        if matched.is_empty() {
            continue;
        }
        if only_matched_once && matched.len() > 1 {
            continue;
        }
        let selector_text = selectors[sel_idx].selector_text.clone();
        let Some(parsed) = parse_selector(&selector_text) else { continue };
        // class names at top level of the selector
        let mut class_names: Vec<String> = Vec::new();
        let mut first_id: Option<String> = None;
        let mut first_simple_is_id = false;
        let mut seen_any = false;
        for compound in &parsed.compounds {
            for simple in &compound.0 {
                if !seen_any {
                    seen_any = true;
                    if let Simple::Id(id) = simple {
                        first_simple_is_id = true;
                        first_id = Some(id.clone());
                    }
                }
                if let Simple::Class(c) = simple {
                    class_names.push(c.clone());
                }
            }
        }
        for &uid in &matched {
            let Some(elem) = element_by_uid_mut(root, uid) else { continue };
            // class
            let mut class_list: Vec<String> = Vec::new();
            if let Some(cls) = elem.get("class") {
                for c in cls.split(' ') {
                    if !class_list.iter().any(|x| x == c) {
                        class_list.push(c.to_string());
                    }
                }
            }
            for classname in &class_names {
                if !all_selector_texts.iter().any(|t| {
                    includes_attr_selector(t, "class", Some(classname), true)
                }) {
                    class_list.retain(|x| x != classname);
                }
            }
            if class_list.is_empty() {
                elem.remove_attr("class");
            } else {
                elem.set("class", &class_list.join(" "));
            }
            // ID
            if first_simple_is_id {
                if let (Some(idname), Some(eid)) =
                    (first_id.clone(), elem.get("id").map(|s| s.to_string()))
                {
                    if eid == idname
                        && !all_selector_texts.iter().any(|t| {
                            includes_attr_selector(t, "id", Some(&idname), true)
                        })
                    {
                        elem.remove_attr("id");
                    }
                }
            }
        }
    }

    // clean up style elements
    let style_infos: Vec<(u64, Vec<CssNode>)> = styles
        .into_iter()
        .map(|mut s| {
            prune_empty_rules(&mut s.nodes);
            (s.uid, s.nodes)
        })
        .collect();
    for (uid, nodes) in style_infos {
        if nodes.is_empty() {
            detach_by_uid(root, uid);
        } else {
            let generated = css::generate(&nodes, false);
            if let Some(e) = element_by_uid_mut(root, uid) {
                if let Some(first) = e.children.first_mut() {
                    match first {
                        Node::Text(v) | Node::Cdata(v) => *v = generated,
                        _ => {}
                    }
                }
            }
        }
    }
}
