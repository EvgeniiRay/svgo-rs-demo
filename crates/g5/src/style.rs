use crate::dom::{Document, Element, Node};
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Rule {
    pub selectors: Vec<String>,
    pub decls: Vec<(String, String)>,
    pub order: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
    /// Parsed inline `style` declarations keyed by the exact `style` string
    /// (svg/svgo@68c0660). Keying by value means an updated attribute can
    /// never return stale declarations; the cache lives with this
    /// stylesheet snapshot, mirroring the upstream per-AST `declarationCache`.
    pub declaration_cache: RefCell<HashMap<String, Vec<(String, String)>>>,
}

impl Stylesheet {
    fn inline_declarations(&self, style: &str) -> Vec<(String, String)> {
        if let Some(cached) = self.declaration_cache.borrow().get(style) {
            return cached.clone();
        }
        let parsed = parse_decls(style);
        self.declaration_cache
            .borrow_mut()
            .insert(style.to_string(), parsed.clone());
        parsed
    }
    /// Number of distinct `style` strings parsed so far (inspection/test).
    pub fn declaration_cache_len(&self) -> usize {
        self.declaration_cache.borrow().len()
    }
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
    Stylesheet { rules, declaration_cache: Default::default() }
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
            for (k, v2) in sheet.inline_declarations(inline) {
                if k == prop {
                    return Some(v2);
                }
            }
        }
        return Some(v);
    }
    if let Some(inline) = el.get("style") {
        for (k, v2) in sheet.inline_declarations(inline) {
            if k == prop {
                return Some(v2);
            }
        }
    }
    el.get(prop).map(|s| s.to_string())
}

pub fn has_filter(sheet: &Stylesheet, el: &Element) -> bool {
    compute_prop(sheet, el, "filter").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el_with_style(style: &str) -> Element {
        Element {
            name: "rect".to_string(),
            attrs: vec![("id".into(), "x".into()), ("style".into(), style.into())],
            children: vec![],
        }
    }

    /// Port of `lib/style.test.js` "caches inline declarations by style
    /// value" (svg/svgo@68c0660): identical `style` strings share one
    /// parse, and a changed string never returns stale declarations.
    #[test]
    fn caches_inline_declarations_by_style_value() {
        let doc = Document { children: vec![] };
        let sheet = collect_stylesheet(&doc);
        let one = el_with_style("fill: red");
        let two = el_with_style("fill: red");
        assert_eq!(compute_prop(&sheet, &one, "fill").as_deref(), Some("red"));
        assert_eq!(compute_prop(&sheet, &two, "fill").as_deref(), Some("red"));
        assert_eq!(sheet.declaration_cache_len(), 1);

        let mut two2 = two.clone();
        two2.set("style", "fill: blue".to_string());
        assert_eq!(compute_prop(&sheet, &two2, "fill").as_deref(), Some("blue"));
        assert_eq!(sheet.declaration_cache_len(), 2);
    }
}

pub fn walk_elements<F: FnMut(&Element)>(nodes: &[Node], f: &mut F) {
    for n in nodes {
        if let Node::Element(el) = n {
            f(el);
            walk_elements(&el.children, f);
        }
    }
}
