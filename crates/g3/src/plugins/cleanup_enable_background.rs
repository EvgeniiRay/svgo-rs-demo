//! Port of plugins/cleanupEnableBackground.js
//!
//! Style values are handled with a minimal declaration-list parser instead
//! of css-tree; see unresolved notes for formatting divergences.

use crate::xast::{any_element, walk_elements, Root};

#[derive(Debug, Clone, PartialEq)]
struct Decl {
    prop: String,
    value: String,
    /// Raw text when the chunk has no `prop:` shape.
    raw: Option<String>,
}

fn parse_style(style: &str) -> Vec<Decl> {
    style
        .split(';')
        .filter(|s| !s.trim().is_empty())
        .map(|chunk| match chunk.split_once(':') {
            Some((p, v)) => Decl { prop: p.trim().to_string(), value: v.trim().to_string(), raw: None },
            None => Decl { prop: String::new(), value: String::new(), raw: Some(chunk.trim().to_string()) },
        })
        .collect()
}

fn generate_style(decls: &[Decl]) -> String {
    let parts: Vec<String> = decls
        .iter()
        .map(|d| match &d.raw {
            Some(r) => r.clone(),
            None => format!("{}:{}", d.prop, d.value),
        })
        .collect();
    parts.join(";")
}

/// ^new\s0\s0\s(num)\s(num)$ ; returns Some(cleaned) / None semantics folded:
/// `cleanup_value` returns the original value when there is no full match,
/// `None` (JS undefined) when the box coincides on an `<svg>`, and
/// `Some("new")` when it coincides on mask/pattern.
fn cleanup_value(value: Option<&str>, node_name: &str, width: &str, height: &str) -> Option<String> {
    let value = value?;
    let rest = match value.strip_prefix("new") {
        Some(r) if r.chars().next().is_some_and(|c| c.is_whitespace()) => r.trim_start(),
        _ => return Some(value.to_string()),
    };
    let toks: Vec<&str> = rest.split_whitespace().collect();
    if toks.len() != 4 || toks[0] != "0" || toks[1] != "0" || !is_number(toks[2]) || !is_number(toks[3]) {
        return Some(value.to_string());
    }
    if width == toks[2] && height == toks[3] {
        if node_name == "svg" { None } else { Some("new".to_string()) }
    } else {
        Some(value.to_string())
    }
}

fn is_number(s: &str) -> bool {
    // [-+]?\d*\.?\d+([eE][-+]?\d+)?
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') { i += 1; }
    let mut digits = 0;
    while i < b.len() && b[i].is_ascii_digit() { i += 1; digits += 1; }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() { i += 1; digits += 1; }
    }
    if digits == 0 { return false; }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') { i += 1; }
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() { i += 1; }
        if i == start { return false; }
    }
    i == b.len()
}

pub fn apply(root: &mut Root) {
    let has_filter = any_element(&root.children, &|el| el.name == "filter");
    walk_elements(&mut root.children, &mut |node| {
        // parse style, dedupe enable-background keeping the last declaration
        let mut decls: Option<Vec<Decl>> = node.get("style").map(|s| {
            let all = parse_style(s);
            let last_eb = all.iter().rposition(|d| d.prop == "enable-background");
            all.into_iter()
                .enumerate()
                .filter(|(i, d)| d.prop != "enable-background" || Some(*i) == last_eb)
                .map(|(_, d)| d)
                .collect()
        });

        if !has_filter {
            node.remove("enable-background");
            if let Some(list) = &mut decls {
                list.retain(|d| d.prop != "enable-background");
                if list.is_empty() {
                    node.remove("style");
                } else {
                    node.set("style", &generate_style(list));
                }
            }
            return;
        }

        let has_dims = node.has("width") && node.has("height");
        if matches!(node.name.as_str(), "svg" | "mask" | "pattern") && has_dims {
            let width = node.get("width").unwrap_or("").to_string();
            let height = node.get("height").unwrap_or("").to_string();
            let attr = node.get("enable-background").map(|s| s.to_string());
            match cleanup_value(attr.as_deref(), &node.name.clone(), &width, &height) {
                Some(v) => node.set("enable-background", &v),
                None => node.remove("enable-background"),
            }
            if let Some(list) = &mut decls {
                if let Some(pos) = list.iter().rposition(|d| d.prop == "enable-background") {
                    let style_value = list[pos].value.clone();
                    match cleanup_value(Some(&style_value), &node.name.clone(), &width, &height) {
                        Some(v) => list[pos].value = v,
                        None => {
                            list.remove(pos);
                        }
                    }
                }
            }
        }

        if let Some(list) = &decls {
            if list.is_empty() {
                node.remove("style");
            } else {
                node.set("style", &generate_style(list));
            }
        }
    });
}
