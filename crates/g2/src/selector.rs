//! CSS selector parsing, matching (css-select subset), specificity and
//! `includesAttrSelector` (css-what subset).

use crate::dom::{Element, Node};

#[derive(Debug, Clone, PartialEq)]
pub enum Simple {
    Tag(String),
    Universal,
    Class(String),
    Id(String),
    Attr { name: String, op: Option<String>, value: Option<String> },
    Pseudo { name: String, arg: Option<String>, element: bool, raw: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Compound(pub Vec<Simple>);

#[derive(Debug, Clone, PartialEq)]
pub struct Selector {
    /// compounds left-to-right; combinators[i] joins compounds[i] -> compounds[i+1]
    pub compounds: Vec<Compound>,
    pub combinators: Vec<char>, // ' ', '>', '+', '~', 'd' for /deep/
}

fn read_ident(chars: &[char], i: &mut usize) -> String {
    let mut s = String::new();
    while *i < chars.len() {
        let c = chars[*i];
        if c.is_alphanumeric() || c == '-' || c == '_' || (c as u32) > 127 || c == '\\' {
            if c == '\\' && *i + 1 < chars.len() {
                *i += 1;
                s.push(chars[*i]);
            } else {
                s.push(c);
            }
            *i += 1;
        } else {
            break;
        }
    }
    s
}

/// Read a balanced `(...)` (assumes chars[i]=='('), returns inner and advances past ')'.
fn read_parens(chars: &[char], i: &mut usize) -> String {
    let mut depth = 0i32;
    let mut inner = String::new();
    let mut quote: Option<char> = None;
    while *i < chars.len() {
        let c = chars[*i];
        *i += 1;
        match quote {
            Some(q) => {
                if depth == 0 {
                    // shouldn't happen
                }
                if c == q {
                    quote = None;
                } else if depth > 0 {
                    inner.push(c);
                }
            }
            None => match c {
                '"' | '\'' => {
                    quote = Some(c);
                    if depth > 0 {
                        inner.push(c);
                    }
                }
                '(' => {
                    depth += 1;
                    if depth > 1 {
                        inner.push(c);
                    }
                }
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    inner.push(c);
                }
                _ => {
                    if depth > 0 {
                        inner.push(c);
                    }
                }
            },
        }
    }
    inner
}

pub fn parse_selector(input: &str) -> Option<Selector> {
    let chars: Vec<char> = input.trim().chars().collect();
    let mut compounds: Vec<Compound> = Vec::new();
    let mut combinators: Vec<char> = Vec::new();
    let mut current: Vec<Simple> = Vec::new();
    let mut i = 0usize;
    let mut pending_space = false;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            pending_space = true;
            i += 1;
            continue;
        }
        // /deep/ combinator
        if chars[i..].iter().collect::<String>().starts_with("/deep/") {
            if !current.is_empty() {
                compounds.push(Compound(std::mem::take(&mut current)));
            }
            combinators.push('d');
            i += 6;
            pending_space = false;
            continue;
        }
        if matches!(c, '>' | '+' | '~') {
            if !current.is_empty() {
                compounds.push(Compound(std::mem::take(&mut current)));
            }
            combinators.push(c);
            i += 1;
            pending_space = false;
            continue;
        }
        if pending_space && !current.is_empty() {
            compounds.push(Compound(std::mem::take(&mut current)));
            combinators.push(' ');
        }
        pending_space = false;
        match c {
            '*' => {
                current.push(Simple::Universal);
                i += 1;
            }
            '.' => {
                i += 1;
                let name = read_ident(&chars, &mut i);
                if name.is_empty() {
                    return None;
                }
                current.push(Simple::Class(name));
            }
            '#' => {
                i += 1;
                let name = read_ident(&chars, &mut i);
                if name.is_empty() {
                    return None;
                }
                current.push(Simple::Id(name));
            }
            '[' => {
                // read to matching ]
                let mut j = i + 1;
                let mut quote: Option<char> = None;
                let mut inner = String::new();
                while j < chars.len() {
                    let cc = chars[j];
                    match quote {
                        Some(q) => {
                            inner.push(cc);
                            if cc == q {
                                quote = None;
                            }
                        }
                        None => {
                            if cc == '"' || cc == '\'' {
                                quote = Some(cc);
                                inner.push(cc);
                            } else if cc == ']' {
                                break;
                            } else {
                                inner.push(cc);
                            }
                        }
                    }
                    j += 1;
                }
                i = j + 1;
                let inner = inner.trim().to_string();
                // split attr op
                let mut name_end = inner.len();
                let mut op: Option<String> = None;
                let mut value: Option<String> = None;
                for (opstr, _l) in [("~=", 2), ("|=", 2), ("^=", 2), ("$=", 2), ("*=", 2), ("=", 1)] {
                    if let Some(pos) = inner.find(opstr) {
                        name_end = pos;
                        op = Some(opstr.to_string());
                        let mut v = inner[pos + opstr.len()..].trim().to_string();
                        // strip case flag like ` i`
                        if (v.ends_with(" i") || v.ends_with(" s")) && v.len() > 2 {
                            // only if quoted or ident before flag; simple check
                            let cand = v[..v.len() - 2].trim().to_string();
                            if cand.starts_with('"') || cand.starts_with('\'') {
                                v = cand;
                            }
                        }
                        if (v.starts_with('"') && v.ends_with('"') && v.len() >= 2)
                            || (v.starts_with('\'') && v.ends_with('\'') && v.len() >= 2)
                        {
                            v = v[1..v.len() - 1].to_string();
                        }
                        value = Some(v);
                        break;
                    }
                }
                let name = inner[..name_end].trim().to_string();
                if name.is_empty() {
                    return None;
                }
                current.push(Simple::Attr { name, op, value });
            }
            ':' => {
                let start = i;
                i += 1;
                let mut element = false;
                if i < chars.len() && chars[i] == ':' {
                    element = true;
                    i += 1;
                }
                let name = read_ident(&chars, &mut i).to_lowercase();
                let arg = if i < chars.len() && chars[i] == '(' {
                    Some(read_parens(&chars, &mut i))
                } else {
                    None
                };
                let raw: String = chars[start..i].iter().collect();
                if name.is_empty() {
                    return None;
                }
                current.push(Simple::Pseudo { name, arg, element, raw });
            }
            _ => {
                let name = read_ident(&chars, &mut i);
                if name.is_empty() {
                    return None;
                }
                current.push(Simple::Tag(name));
            }
        }
    }
    if !current.is_empty() {
        compounds.push(Compound(current));
    }
    if compounds.is_empty() || combinators.len() != compounds.len().saturating_sub(1) {
        // tolerate stray combinator counts
        while combinators.len() > compounds.len().saturating_sub(1) {
            combinators.pop();
        }
        if compounds.is_empty() {
            return None;
        }
    }
    Some(Selector { compounds, combinators })
}

// ---------- matching ----------
pub fn element_at<'a>(nodes: &'a [Node], path: &[usize]) -> Option<&'a Element> {
    crate::dom::get_element(nodes, path)
}

pub fn all_element_paths(nodes: &[Node]) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    fn walk(nodes: &[Node], prefix: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        for (i, n) in nodes.iter().enumerate() {
            if let Node::Element(e) = n {
                prefix.push(i);
                out.push(prefix.clone());
                walk(&e.children, prefix, out);
                prefix.pop();
            }
        }
    }
    walk(nodes, &mut Vec::new(), &mut out);
    out
}

fn parent_path(path: &[usize]) -> Option<Vec<usize>> {
    if path.len() <= 1 {
        // parent is root (no element)
        None
    } else {
        Some(path[..path.len() - 1].to_vec())
    }
}

fn sibling_element_paths(nodes: &[Node], path: &[usize]) -> Vec<Vec<usize>> {
    let (parent_children, self_idx): (&[Node], usize) = if path.len() == 1 {
        (nodes, path[0])
    } else {
        match element_at(nodes, &path[..path.len() - 1]) {
            Some(parent) => (&parent.children, path[path.len() - 1]),
            None => return Vec::new(),
        }
    };
    let _ = self_idx;
    let mut out = Vec::new();
    let prefix = &path[..path.len() - 1];
    for (i, n) in parent_children.iter().enumerate() {
        if matches!(n, Node::Element(_)) {
            let mut p = prefix.to_vec();
            p.push(i);
            out.push(p);
        }
    }
    out
}

fn match_pseudo(
    nodes: &[Node],
    path: &[usize],
    elem: &Element,
    name: &str,
    arg: &Option<String>,
    element_pseudo: bool,
) -> bool {
    if element_pseudo {
        return false;
    }
    match name {
        "first-child" => sibling_element_paths(nodes, path).first() == Some(&path.to_vec()),
        "last-child" => sibling_element_paths(nodes, path).last() == Some(&path.to_vec()),
        "only-child" => sibling_element_paths(nodes, path).len() == 1,
        "first-of-type" => sibling_element_paths(nodes, path)
            .iter()
            .find(|p| element_at(nodes, p).map(|e| e.name == elem.name).unwrap_or(false))
            == Some(&path.to_vec()),
        "last-of-type" => sibling_element_paths(nodes, path)
            .iter()
            .rev()
            .find(|p| element_at(nodes, p).map(|e| e.name == elem.name).unwrap_or(false))
            == Some(&path.to_vec()),
        "only-of-type" => sibling_element_paths(nodes, path)
            .iter()
            .filter(|p| element_at(nodes, p).map(|e| e.name == elem.name).unwrap_or(false))
            .count()
            == 1,
        "empty" => elem.children.is_empty(),
        "root" => path.len() == 1,
        "nth-child" | "nth-last-child" | "nth-of-type" | "nth-last-of-type" => {
            let Some(arg) = arg else { return false };
            let siblings = sibling_element_paths(nodes, path);
            let filtered: Vec<&Vec<usize>> = if name.contains("of-type") {
                siblings
                    .iter()
                    .filter(|p| element_at(nodes, p).map(|e| e.name == elem.name).unwrap_or(false))
                    .collect()
            } else {
                siblings.iter().collect()
            };
            let pos = filtered.iter().position(|p| *p == &path.to_vec());
            let Some(pos) = pos else { return false };
            let index = if name.contains("last") {
                filtered.len() - pos // 1-based from end
            } else {
                pos + 1
            };
            nth_matches(arg, index)
        }
        "not" => {
            let Some(arg) = arg else { return false };
            // arg is a selector list; element must match none
            !crate::css::split_top_level(arg, ',').iter().any(|sel| {
                matches_selector(nodes, path, sel.trim())
            })
        }
        "is" | "where" => {
            let Some(arg) = arg else { return false };
            crate::css::split_top_level(arg, ',').iter().any(|sel| {
                matches_selector(nodes, path, sel.trim())
            })
        }
        "has" => {
            let Some(arg) = arg else { return false };
            // descendant (or relative) match inside this element
            let mut descendants = Vec::new();
            fn collect(nodes: &[Node], prefix: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
                for (i, n) in nodes.iter().enumerate() {
                    if let Node::Element(e) = n {
                        prefix.push(i);
                        out.push(prefix.clone());
                        collect(&e.children, prefix, out);
                        prefix.pop();
                    }
                }
            }
            collect(&elem.children, &mut path.to_vec(), &mut descendants);
            descendants.iter().any(|p| {
                crate::css::split_top_level(arg, ',').iter().any(|sel| {
                    let sel = sel.trim().trim_start_matches('>').trim();
                    matches_selector(nodes, p, sel)
                })
            })
        }
        _ => false, // dynamic / unknown pseudos never match statically
    }
}

fn nth_matches(arg: &str, index: usize) -> bool {
    let a = arg.trim().to_lowercase().replace(' ', "");
    if a == "odd" {
        return index % 2 == 1;
    }
    if a == "even" {
        return index % 2 == 0;
    }
    if let Ok(n) = a.parse::<i64>() {
        return index as i64 == n;
    }
    // an+b
    if let Some(npos) = a.find('n') {
        let coef = &a[..npos];
        let coef: i64 = match coef {
            "" | "+" => 1,
            "-" => -1,
            _ => coef.parse().unwrap_or(0),
        };
        let rest = &a[npos + 1..];
        let b: i64 = if rest.is_empty() { 0 } else { rest.parse().unwrap_or(0) };
        if coef == 0 {
            return index as i64 == b;
        }
        let diff = index as i64 - b;
        diff % coef == 0 && diff / coef >= 0
    } else {
        false
    }
}

fn match_compound(nodes: &[Node], path: &[usize], compound: &Compound) -> bool {
    let Some(elem) = element_at(nodes, path) else { return false };
    for simple in &compound.0 {
        let ok = match simple {
            Simple::Universal => true,
            Simple::Tag(t) => &elem.name == t,
            Simple::Class(c) => elem
                .get("class")
                .map(|v| v.split(' ').any(|x| x == c))
                .unwrap_or(false),
            Simple::Id(id) => elem.get("id") == Some(id.as_str()),
            Simple::Attr { name, op, value } => match (op, value) {
                (None, _) => elem.has(name),
                (Some(op), Some(val)) => {
                    let Some(actual) = elem.get(name) else { return false };
                    match op.as_str() {
                        "=" => actual == val,
                        "~=" => actual.split_whitespace().any(|x| x == val),
                        "|=" => actual == val || actual.starts_with(&format!("{val}-")),
                        "^=" => !val.is_empty() && actual.starts_with(val.as_str()),
                        "$=" => !val.is_empty() && actual.ends_with(val.as_str()),
                        "*=" => !val.is_empty() && actual.contains(val.as_str()),
                        _ => false,
                    }
                }
                _ => false,
            },
            Simple::Pseudo { name, arg, element, .. } => {
                match_pseudo(nodes, path, elem, name, arg, *element)
            }
        };
        if !ok {
            return false;
        }
    }
    true
}

pub fn matches_parsed(nodes: &[Node], path: &[usize], sel: &Selector) -> bool {
    fn rec(nodes: &[Node], path: &[usize], sel: &Selector, idx: usize) -> bool {
        if !match_compound(nodes, path, &sel.compounds[idx]) {
            return false;
        }
        if idx == 0 {
            return true;
        }
        let comb = sel.combinators[idx - 1];
        match comb {
            '>' => match parent_path(path) {
                Some(p) => rec(nodes, &p, sel, idx - 1),
                None => false,
            },
            ' ' | 'd' => {
                let mut cur = parent_path(path);
                while let Some(p) = cur {
                    if rec(nodes, &p, sel, idx - 1) {
                        return true;
                    }
                    cur = parent_path(&p);
                }
                false
            }
            '+' => {
                let sibs = sibling_element_paths(nodes, path);
                match sibs.iter().position(|p| p == path) {
                    Some(pos) if pos > 0 => rec(nodes, &sibs[pos - 1], sel, idx - 1),
                    _ => false,
                }
            }
            '~' => {
                let sibs = sibling_element_paths(nodes, path);
                match sibs.iter().position(|p| p == path) {
                    Some(pos) => (0..pos).rev().any(|k| rec(nodes, &sibs[k], sel, idx - 1)),
                    _ => false,
                }
            }
            _ => false,
        }
    }
    if sel.compounds.is_empty() {
        return false;
    }
    rec(nodes, path, sel, sel.compounds.len() - 1)
}

pub fn matches_selector(nodes: &[Node], path: &[usize], selector_text: &str) -> bool {
    // top-level comma list (querySelectorAll semantics for a single selector
    // string without commas in SVGO use, but tolerate lists)
    for part in crate::css::split_top_level(selector_text, ',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(sel) = parse_selector(part) {
            if matches_parsed(nodes, path, &sel) {
                return true;
            }
        }
    }
    false
}

/// Specificity as a comparable (ids, classes, types) triple.
/// Functional `:not/:is/:has` contribute their most specific argument;
/// `:where` contributes nothing.
pub fn specificity(selector_text: &str) -> (u32, u32, u32) {
    let Some(sel) = parse_selector(selector_text) else { return (0, 0, 0) };
    let mut spec = (0u32, 0u32, 0u32);
    for compound in &sel.compounds {
        for simple in &compound.0 {
            match simple {
                Simple::Id(_) => spec.0 += 1,
                Simple::Class(_) | Simple::Attr { .. } => spec.1 += 1,
                Simple::Tag(_) => spec.2 += 1,
                Simple::Pseudo { name, arg, element, .. } => {
                    if *element {
                        spec.2 += 1;
                    } else if matches!(name.as_str(), "not" | "is" | "has") {
                        if let Some(arg) = arg {
                            let best = crate::css::split_top_level(arg, ',')
                                .iter()
                                .map(|s| specificity(s.trim()))
                                .max()
                                .unwrap_or((0, 0, 0));
                            spec.0 += best.0;
                            spec.1 += best.1;
                            spec.2 += best.2;
                        }
                    } else if name == "where" {
                        // zero
                    } else {
                        spec.1 += 1;
                    }
                }
                Simple::Universal => {}
            }
        }
    }
    spec
}

/// Flattened token view for `includesAttrSelector`: each simple selector,
/// terminated by a marker when a combinator follows its compound.
enum FlatTok {
    Attr(String, Option<String>), // name, value (class/id mapped to attr form)
    Other,
    Traversal,
}

fn flatten(sel: &Selector) -> Vec<FlatTok> {
    let mut out = Vec::new();
    for (ci, compound) in sel.compounds.iter().enumerate() {
        for simple in &compound.0 {
            match simple {
                Simple::Class(c) => out.push(FlatTok::Attr("class".into(), Some(c.clone()))),
                Simple::Id(id) => out.push(FlatTok::Attr("id".into(), Some(id.clone()))),
                Simple::Attr { name, value, .. } => {
                    out.push(FlatTok::Attr(name.clone(), value.clone()))
                }
                _ => out.push(FlatTok::Other),
            }
        }
        if ci < sel.combinators.len() {
            out.push(FlatTok::Traversal);
        }
    }
    out
}

/// Port of `includesAttrSelector` from `lib/style.js`.
pub fn includes_attr_selector(
    selector_text: &str,
    name: &str,
    value: Option<&str>,
    traversed: bool,
) -> bool {
    for part in crate::css::split_top_level(selector_text, ',') {
        let Some(sel) = parse_selector(part.trim()) else { continue };
        let toks = flatten(&sel);
        for (idx, tok) in toks.iter().enumerate() {
            if traversed {
                if idx == toks.len() - 1 {
                    continue;
                }
                if !matches!(toks.get(idx + 1), Some(FlatTok::Traversal)) {
                    continue;
                }
            }
            if let FlatTok::Attr(n, v) = tok {
                if n == name && (value.is_none() || v.as_deref() == value) {
                    return true;
                }
            }
        }
    }
    false
}

/// querySelectorAll: uids of elements matching `selector_text`, document order.
pub fn query_selector_all(nodes: &[Node], selector_text: &str) -> Vec<u64> {
    let mut out = Vec::new();
    for path in all_element_paths(nodes) {
        if matches_selector(nodes, &path, selector_text) {
            if let Some(e) = element_at(nodes, &path) {
                out.push(e.uid);
            }
        }
    }
    out
}
