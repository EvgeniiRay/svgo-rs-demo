use crate::dom::{Document, Element, Node};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct Rule {
    pub selectors: Vec<String>,
    pub decls: Vec<(String, String)>,
    pub order: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
    /// Parsed inline `style` declarations keyed by the exact style string
    /// (port of the per-AST declaration cache in upstream `lib/style.js`).
    /// Keying by value means an updated attribute can never hit stale data.
    pub declaration_cache: RefCell<HashMap<String, Rc<Vec<(String, String)>>>>,
}

/// Returns the parsed declarations for an inline `style` string, reusing the
/// stylesheet's cache when the same string was parsed before.
pub fn inline_declarations(sheet: &Stylesheet, style: &str) -> Rc<Vec<(String, String)>> {
    if let Some(cached) = sheet.declaration_cache.borrow().get(style) {
        return Rc::clone(cached);
    }
    let parsed = Rc::new(parse_decls(style));
    sheet.declaration_cache
        .borrow_mut()
        .insert(style.to_string(), Rc::clone(&parsed));
    parsed
}

/// Number of distinct inline style strings parsed into the cache so far.
pub fn declaration_cache_len(sheet: &Stylesheet) -> usize {
    sheet.declaration_cache.borrow().len()
}

fn parse_decls(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for part in body.split(';') {
        if let Some((k, v)) = part.split_once(':') {
            let k = k.trim().to_string();
            let v = v.trim().trim_end_matches("!important").trim().to_string();
            if !k.is_empty() {
                out.push((k, v));
            }
        }
    }
    out
}

pub fn collect_stylesheet(doc: &Document) -> Stylesheet {
    let mut texts = Vec::new();
    crate::dom::collect_style_texts(&doc.children, &mut texts);
    let mut rules = Vec::new();
    let mut order = 0;
    for text in &texts {
        // strip CSS comments
        let mut clean = String::new();
        let mut rest = text.as_str();
        while let Some(i) = rest.find("/*") {
            clean.push_str(&rest[..i]);
            match rest[i + 2..].find("*/") {
                Some(j) => rest = &rest[i + 2 + j + 2..],
                None => { rest = ""; break; }
            }
        }
        clean.push_str(rest);
        let mut idx = 0;
        let chars: Vec<char> = clean.chars().collect();
        while idx < chars.len() {
            // find '{'
            let mut sel = String::new();
            while idx < chars.len() && chars[idx] != '{' {
                sel.push(chars[idx]);
                idx += 1;
            }
            if idx >= chars.len() { break; }
            idx += 1;
            let mut body = String::new();
            let mut depth = 1;
            while idx < chars.len() && depth > 0 {
                if chars[idx] == '{' { depth += 1; }
                if chars[idx] == '}' {
                    depth -= 1;
                    if depth == 0 { idx += 1; break; }
                }
                body.push(chars[idx]);
                idx += 1;
            }
            let sel = sel.trim().to_string();
            if sel.is_empty() || sel.starts_with('@') { continue; }
            let selectors: Vec<String> = sel.split(',').map(|s| s.trim().to_string()).collect();
            rules.push(Rule { selectors, decls: parse_decls(&body), order });
            order += 1;
        }
    }
    Stylesheet {
        rules,
        declaration_cache: RefCell::new(HashMap::new()),
    }
}

fn selector_matches(sel: &str, el: &Element) -> Option<i32> {
    // Supports: *, tag, .cls, #id, tag.cls, tag#id, .a.b, descendant combinators (approx: last part)
    let parts: Vec<&str> = sel.split_whitespace().collect();
    let last = match parts.last() {
        Some(p) => *p,
        None => return None,
    };
    // pseudo / attribute selectors: not supported
    if last.contains(':') || last.contains('[') {
        return None;
    }
    let mut specificity = 0;
    let (tag, rest) = {
        let mut split_at = last.len();
        for (i, c) in last.char_indices() {
            if c == '.' || c == '#' { split_at = i; break; }
        }
        (&last[..split_at], &last[split_at..])
    };
    if !tag.is_empty() && tag != "*" && tag != el.name {
        return None;
    }
    if !tag.is_empty() && tag != "*" {
        specificity += 1;
    }
    // verify ancestor for descendant selector (rough): ignore beyond last
    let mut rest_chars = rest;
    while !rest_chars.is_empty() {
        if let Some(stripped) = rest_chars.strip_prefix('.') {
            let name: String = stripped.chars().take_while(|c| *c != '.' && *c != '#').collect();
            let classes: Vec<&str> = el.get("class").unwrap_or("").split_whitespace().collect();
            if !classes.contains(&name.as_str()) { return None; }
            specificity += 10;
            rest_chars = &stripped[name.len()..];
        } else if let Some(stripped) = rest_chars.strip_prefix('#') {
            let name: String = stripped.chars().take_while(|c| *c != '.' && *c != '#').collect();
            if el.get("id") != Some(name.as_str()) { return None; }
            specificity += 100;
            rest_chars = &stripped[name.len()..];
        } else {
            return None;
        }
    }
    Some(specificity)
}

/// Computed value for a property: presentation attribute < stylesheet < inline style.
/// Returns None if absent. `!important` handling is simplified.
pub fn compute_prop(sheet: &Stylesheet, el: &Element, prop: &str) -> Option<String> {
    let mut best: Option<(i32, usize, String)> = None;
    for rule in &sheet.rules {
        for sel in &rule.selectors {
            if let Some(spec) = selector_matches(sel, el) {
                for (k, v) in &rule.decls {
                    if k == prop {
                        let replace = match &best {
                            None => true,
                            Some((s, o, _)) => spec > *s || (spec == *s && rule.order >= *o),
                        };
                        if replace {
                            best = Some((spec, rule.order, v.clone()));
                        }
                    }
                }
            }
        }
    }
    if let Some((_, _, v)) = best {
        // inline style wins over stylesheet
        if let Some(inline) = el.get("style") {
            for (k, v2) in inline_declarations(sheet, inline).iter() {
                if k == prop {
                    return Some(v2.clone());
                }
            }
        }
        return Some(v);
    }
    if let Some(inline) = el.get("style") {
        for (k, v2) in inline_declarations(sheet, inline).iter() {
            if k == prop {
                return Some(v2.clone());
            }
        }
    }
    el.get(prop).map(|s| s.to_string())
}

pub fn has_filter(sheet: &Stylesheet, el: &Element) -> bool {
    compute_prop(sheet, el, "filter").is_some()
}

pub fn walk_elements<F: FnMut(&Element)>(nodes: &[Node], f: &mut F) {
    for n in nodes {
        if let Node::Element(el) = n {
            f(el);
            walk_elements(&el.children, f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find_by_id<'a>(nodes: &'a [Node], id: &str) -> Option<&'a Element> {
        for n in nodes {
            if let Node::Element(el) = n {
                if el.get("id") == Some(id) {
                    return Some(el);
                }
                if let Some(found) = find_by_id(&el.children, id) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn find_by_id_mut<'a>(nodes: &'a mut [Node], id: &str) -> Option<&'a mut Element> {
        for n in nodes {
            if let Node::Element(el) = n {
                if el.get("id") == Some(id) {
                    return Some(el);
                }
                if let Some(found) = find_by_id_mut(&mut el.children, id) {
                    return Some(found);
                }
            }
        }
        None
    }

    // Port of upstream `lib/style.test.js`: "caches inline declarations by style value".
    #[test]
    fn caches_inline_declarations_by_style_value() {
        let mut doc = crate::dom::parse(
            "<svg><rect id=\"one\" style=\"fill: red\" /><rect id=\"two\" style=\"fill: red\" /></svg>",
        );
        let sheet = collect_stylesheet(&doc);
        let one = find_by_id(&doc.children, "one").unwrap().clone();
        let two = find_by_id(&doc.children, "two").unwrap().clone();
        assert_eq!(compute_prop(&sheet, &one, "fill").as_deref(), Some("red"));
        assert_eq!(compute_prop(&sheet, &two, "fill").as_deref(), Some("red"));
        assert_eq!(declaration_cache_len(&sheet), 1);

        find_by_id_mut(&mut doc.children, "two")
            .unwrap()
            .set("style", "fill: blue".to_string());
        let two = find_by_id(&doc.children, "two").unwrap().clone();
        assert_eq!(compute_prop(&sheet, &two, "fill").as_deref(), Some("blue"));
        assert_eq!(declaration_cache_len(&sheet), 2);
    }
}
