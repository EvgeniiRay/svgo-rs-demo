//! Lead-ported preset-default plugins not covered by the six group
//! agents: removeDeprecatedAttrs, removeUnknownsAndDefaults, sortAttrs,
//! sortDefsChildren. They operate on the shared g6 XAST DOM. Element /
//! attribute tables come from `lead_data.txt`, generated from the JS
//! `_collections.js` by `scripts/generate_lead_data.mjs`.

use g6::xast::{XastChild, XastElement, XastRoot};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

struct Tables {
    elems: HashMap<String, ElemInfo>,
    deprecated_groups: HashMap<String, (Vec<String>, Vec<String>)>,
    presentation: HashSet<String>,
    inheritable: HashSet<String>,
    non_inheritable_group: HashSet<String>,
}

#[derive(Default)]
struct ElemInfo {
    allowed_attrs: HashSet<String>,
    defaults: HashMap<String, String>,
    allowed_children: HashSet<String>,
    has_entry: bool,
    groups: Vec<String>,
    deprecated_safe: Vec<String>,
    deprecated_unsafe: Vec<String>,
}

fn csv(s: &str) -> Vec<String> {
    if s.is_empty() { Vec::new() } else { s.split(',').map(|x| x.to_string()).collect() }
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = Tables {
            elems: HashMap::new(),
            deprecated_groups: HashMap::new(),
            presentation: HashSet::new(),
            inheritable: HashSet::new(),
            non_inheritable_group: HashSet::new(),
        };
        for line in include_str!("lead_data.txt").lines() {
            let f: Vec<&str> = line.split('\t').collect();
            match f[0] {
                "E" => {
                    let e = t.elems.entry(f[1].to_string()).or_default();
                    e.has_entry = true;
                    e.allowed_attrs = csv(f[2]).into_iter().collect();
                    e.allowed_children = csv(f[4]).into_iter().collect();
                    for pair in f[3].split(';').filter(|p| !p.is_empty()) {
                        if let Some((k, v)) = pair.split_once('\u{1f}') {
                            e.defaults.insert(k.to_string(), v.to_string());
                        }
                    }
                }
                "D" => {
                    let e = t.elems.entry(f[1].to_string()).or_default();
                    e.groups = csv(f[2]);
                    e.deprecated_safe = csv(f[3]);
                    e.deprecated_unsafe = csv(f[4]);
                }
                "G" => {
                    t.deprecated_groups.insert(f[1].to_string(), (csv(f[2]), csv(f[3])));
                }
                "P" => t.presentation = csv(f[1]).into_iter().collect(),
                "I" => t.inheritable = csv(f[1]).into_iter().collect(),
                "N" => t.non_inheritable_group = csv(f[1]).into_iter().collect(),
                _ => {}
            }
        }
        t
    })
}

// ---------- stylesheet helpers (subset of lib/style.js) ----------

struct Rule {
    selectors: Vec<String>,
    declarations: Vec<(String, String)>,
}

fn collect_rules(root: &XastRoot) -> Vec<Rule> {
    let mut texts = Vec::new();
    fn walk(children: &[XastChild], out: &mut Vec<String>) {
        for c in children {
            if let XastChild::Element(el) = c {
                if el.name == "style" {
                    let mut s = String::new();
                    for ch in &el.children {
                        if let XastChild::Text { value } | XastChild::Cdata { value } = ch { s.push_str(value); }
                    }
                    out.push(s);
                }
                walk(&el.children, out);
            }
        }
    }
    walk(&root.children, &mut texts);
    let mut rules = Vec::new();
    for css in texts {
        let mut rest = css.as_str();
        while let Some(open) = rest.find('{') {
            let selector = rest[..open].trim().to_string();
            let Some(close) = rest[open..].find('}') else { break };
            let body = &rest[open + 1..open + close];
            let declarations = body
                .split(';')
                .filter_map(|d| d.split_once(':'))
                .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                .collect();
            if !selector.is_empty() {
                rules.push(Rule {
                    selectors: selector.split(',').map(|s| s.trim().to_string()).collect(),
                    declarations,
                });
            }
            rest = &rest[open + close + 1..];
        }
    }
    rules
}

/// Attribute names referenced by attribute selectors in a selector,
/// mirroring css-what parsing (`.x` counts as class, `#x` as id).
fn attr_selector_names(selector: &str) -> Vec<String> {
    let mut out = Vec::new();
    let chars: Vec<char> = selector.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '[' => {
                let mut name = String::new();
                i += 1;
                while i < chars.len() && (chars[i].is_alphanumeric() || "-_".contains(chars[i])) {
                    name.push(chars[i]);
                    i += 1;
                }
                if !name.is_empty() { out.push(name); }
            }
            '.' => { out.push("class".to_string()); i += 1; }
            '#' => { out.push("id".to_string()); i += 1; }
            _ => i += 1,
        }
    }
    out
}

fn stylesheet_attr_names(rules: &[Rule]) -> HashSet<String> {
    let mut out = HashSet::new();
    for r in rules {
        for s in &r.selectors {
            for n in attr_selector_names(s) { out.insert(n); }
        }
    }
    out
}

fn selector_includes_attr(rules: &[Rule], name: &str) -> bool {
    rules.iter().any(|r| r.selectors.iter().any(|s| attr_selector_names(s).iter().any(|n| n == name)))
}

/// Match a (possibly descendant) selector against an element + ancestors.
fn selector_matches(selector: &str, el: &XastElement, ancestors: &[XastElement]) -> bool {
    let compounds: Vec<&str> = selector.split(|c: char| c.is_whitespace() || c == '>').filter(|s| !s.is_empty()).collect();
    let Some((last, rest)) = compounds.split_last() else { return false };
    if !compound_matches(last, el) { return false }
    let mut ai = ancestors.len();
    for comp in rest.iter().rev() {
        let mut found = false;
        while ai > 0 {
            ai -= 1;
            if compound_matches(comp, &ancestors[ai]) { found = true; break; }
        }
        if !found { return false }
    }
    true
}

fn compound_matches(compound: &str, el: &XastElement) -> bool {
    let chars: Vec<char> = compound.chars().collect();
    let mut i = 0;
    // tag name
    let mut tag = String::new();
    while i < chars.len() && (chars[i].is_alphanumeric() || "-_*".contains(chars[i])) {
        tag.push(chars[i]);
        i += 1;
    }
    if !tag.is_empty() && tag != "*" && tag != el.name { return false }
    while i < chars.len() {
        match chars[i] {
            '.' => {
                i += 1;
                let mut cls = String::new();
                while i < chars.len() && (chars[i].is_alphanumeric() || "-_".contains(chars[i])) { cls.push(chars[i]); i += 1; }
                let classes = el.get_attr("class").unwrap_or("");
                if !classes.split_whitespace().any(|c| c == cls) { return false }
            }
            '#' => {
                i += 1;
                let mut id = String::new();
                while i < chars.len() && (chars[i].is_alphanumeric() || "-_".contains(chars[i])) { id.push(chars[i]); i += 1; }
                if el.get_attr("id") != Some(id.as_str()) { return false }
            }
            '[' => {
                let mut inner = String::new();
                i += 1;
                while i < chars.len() && chars[i] != ']' { inner.push(chars[i]); i += 1; }
                i += 1;
                let (name, value) = match inner.split_once('=') {
                    Some((n, v)) => (n.trim().to_string(), Some(v.trim().trim_matches(|c| c == '"' || c == '\'').to_string())),
                    None => (inner.trim().to_string(), None),
                };
                match el.get_attr(&name) {
                    None => return false,
                    Some(v) => if let Some(want) = value { if v != want { return false } },
                }
            }
            ':' => return false, // dynamic pseudo-class: never matches statically
            _ => i += 1,
        }
    }
    true
}

fn parse_style_attr(style: &str) -> Vec<(String, String)> {
    style.split(';').filter_map(|d| d.split_once(':')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect()
}

/// computeStyle subset: presentation attrs + matching rules + inline
/// style, with inheritable values flowing from ancestors.
fn compute_style(rules: &[Rule], el: &XastElement, ancestors: &[XastElement]) -> HashMap<String, (bool, String)> {
    // value: (is_static, value)
    fn own(rules: &[Rule], el: &XastElement, ancestors: &[XastElement], t: &Tables) -> HashMap<String, (bool, String)> {
        let mut out: HashMap<String, (bool, String)> = HashMap::new();
        for (n, v) in &el.attributes {
            if t.presentation.contains(n) { out.insert(n.clone(), (true, v.clone())); }
        }
        for r in rules {
            if r.selectors.iter().any(|s| selector_matches(s, el, ancestors)) {
                for (n, v) in &r.declarations {
                    out.insert(n.clone(), (true, v.clone()));
                }
            }
        }
        if let Some(style) = el.get_attr("style") {
            for (n, v) in parse_style_attr(style) { out.insert(n, (true, v)); }
        }
        out
    }
    let t = tables();
    let mut out = own(rules, el, ancestors, t);
    for (i, anc) in ancestors.iter().enumerate().rev() {
        let anc_own = own(rules, anc, &ancestors[..i], t);
        for (n, v) in anc_own {
            if !out.contains_key(&n) && t.inheritable.contains(&n) && !t.non_inheritable_group.contains(&n) {
                out.insert(n, v);
            }
        }
    }
    out
}

// ---------- removeDeprecatedAttrs ----------

pub fn remove_deprecated_attrs(root: &mut XastRoot, remove_unsafe: bool) {
    let rules = collect_rules(root);
    let in_sheet = stylesheet_attr_names(&rules);
    fn walk(children: &mut [XastChild], in_sheet: &HashSet<String>, remove_unsafe: bool) {
        for c in children.iter_mut() {
            if let XastChild::Element(el) = c {
                let t = tables();
                if let Some(info) = t.elems.get(&el.name) {
                    if info.groups.iter().any(|g| g == "core")
                        && el.get_attr("xml:lang").is_some()
                        && el.get_attr("lang").is_some()
                        && !in_sheet.contains("xml:lang")
                    {
                        el.remove_attr("xml:lang");
                    }
                    let mut remove: Vec<String> = Vec::new();
                    let consider = |safe: &[String], unsafe_: &[String], remove: &mut Vec<String>| {
                        for n in safe { if !in_sheet.contains(n) { remove.push(n.clone()); } }
                        if remove_unsafe {
                            for n in unsafe_ { if !in_sheet.contains(n) { remove.push(n.clone()); } }
                        }
                    };
                    for g in &info.groups {
                        if let Some((safe, unsafe_)) = t.deprecated_groups.get(g) {
                            let (safe, unsafe_) = (safe.clone(), unsafe_.clone());
                            consider(&safe, &unsafe_, &mut remove);
                        }
                    }
                    let (safe, unsafe_) = (info.deprecated_safe.clone(), info.deprecated_unsafe.clone());
                    consider(&safe, &unsafe_, &mut remove);
                    for n in remove { el.remove_attr(&n); }
                }
                walk(&mut el.children, in_sheet, remove_unsafe);
            }
        }
    }
    walk(&mut root.children, &in_sheet, remove_unsafe);
}

// ---------- removeUnknownsAndDefaults ----------

pub struct UnknownsParams {
    pub unknown_content: bool,
    pub unknown_attrs: bool,
    pub default_attrs: bool,
    pub default_markup_declarations: bool,
    pub useless_overrides: bool,
    pub keep_data_attrs: bool,
    pub keep_aria_attrs: bool,
    pub keep_role_attr: bool,
}

impl Default for UnknownsParams {
    fn default() -> Self {
        Self {
            unknown_content: true, unknown_attrs: true, default_attrs: true,
            default_markup_declarations: true, useless_overrides: true,
            keep_data_attrs: true, keep_aria_attrs: true, keep_role_attr: false,
        }
    }
}

pub fn remove_unknowns_and_defaults(root: &mut XastRoot, params: &UnknownsParams) {
    if params.default_markup_declarations {
        for c in root.children.iter_mut() {
            if let XastChild::Instruction { value, .. } = c {
                // JS: value.replace(/\s*standalone\s*=\s*(["'])no\1/, '')
                for quote in ['"', '\''] {
                    let pat = format!("standalone={quote}no{quote}");
                    if let Some(pos) = value.find(&pat) {
                        let mut start = pos;
                        while start > 0 && value.as_bytes()[start - 1].is_ascii_whitespace() { start -= 1; }
                        // also consume whitespace before `standalone` only if it
                        // directly precedes it (approximation of \s*standalone)
                        value.replace_range(start..pos + pat.len(), "");
                        break;
                    }
                }
            }
        }
    }
    let rules = collect_rules(root);
    fn process_children(
        children: &mut Vec<XastChild>,
        parent_name: Option<&str>,
        ancestors: &mut Vec<XastElement>,
        rules: &[Rule],
        params: &UnknownsParams,
    ) {
        let t = tables();
        let mut i = 0;
        while i < children.len() {
            let XastChild::Element(el) = &children[i] else { i += 1; continue };
            let el = el.clone();
            // JS: namespaced elements return before any processing
            let skip_attrs = el.name.contains(':');
            let is_foreign = el.name == "foreignObject";
            // unknown content removal (needs parent element)
            if params.unknown_content && !skip_attrs {
                if let Some(pname) = parent_name {
                    let parent_info = t.elems.get(pname);
                    let allowed = parent_info.map(|p| &p.allowed_children);
                    let remove = match allowed {
                        None => !t.elems.contains_key(&el.name),
                        Some(set) if set.is_empty() => !t.elems.contains_key(&el.name),
                        Some(set) => !set.contains(&el.name),
                    };
                    if remove {
                        children.remove(i);
                        continue;
                    }
                }
            }
            // attribute processing on the live element
            let parent_style = ancestors.last().map(|p| compute_style(rules, p, &ancestors[..ancestors.len() - 1]));
            let XastChild::Element(live) = &mut children[i] else { unreachable!() };
            let info = t.elems.get(&live.name);
            let has_id = live.get_attr("id").is_some();
            let mut remove: Vec<String> = Vec::new();
            for (name, value) in if skip_attrs { Vec::new() } else { live.attributes.clone() } {
                if params.keep_data_attrs && name.starts_with("data-") { continue; }
                if params.keep_aria_attrs && name.starts_with("aria-") { continue; }
                if params.keep_role_attr && name == "role" { continue; }
                if name == "xmlns" { continue; }
                if name.contains(':') {
                    let prefix = name.split(':').next().unwrap_or("");
                    if prefix != "xml" && prefix != "xlink" { continue; }
                }
                if params.unknown_attrs {
                    if let Some(info) = info {
                        if info.has_entry && !info.allowed_attrs.contains(&name) {
                            remove.push(name.clone());
                            continue;
                        }
                    }
                }
                if params.default_attrs && !has_id {
                    if let Some(info) = info {
                        if info.defaults.get(&name) == Some(&value) {
                            let parent_has = parent_style.as_ref().is_some_and(|s| s.contains_key(&name));
                            if !parent_has && !selector_includes_attr(rules, &name) {
                                remove.push(name.clone());
                                continue;
                            }
                        }
                    }
                }
                if params.useless_overrides && !has_id {
                    if !t.non_inheritable_group.contains(&name) {
                        if let Some(style) = &parent_style {
                            if let Some((is_static, v)) = style.get(&name) {
                                if *is_static && *v == value {
                                    remove.push(name.clone());
                                }
                            }
                        }
                    }
                }
            }
            for n in remove { live.remove_attr(&n); }
            if is_foreign {
                i += 1;
                continue; // visitSkip: do not process subtree
            }
            // recurse
            let XastChild::Element(live) = &mut children[i] else { unreachable!() };
            let name = live.name.clone();
            let snapshot = live.clone();
            ancestors.push(snapshot);
            process_children(&mut live.children, Some(&name), ancestors, rules, params);
            ancestors.pop();
            i += 1;
        }
    }
    let mut ancestors: Vec<XastElement> = Vec::new();
    process_children(&mut root.children, None, &mut ancestors, &rules, params);
}

// ---------- sortAttrs ----------

pub fn sort_attrs(root: &mut XastRoot, order: Option<Vec<String>>, xmlns_front: bool) {
    let order = order.unwrap_or_else(|| {
        ["id", "width", "height", "x", "x1", "x2", "y", "y1", "y2", "cx", "cy", "r", "fill", "stroke", "marker", "d", "points"]
            .iter().map(|s| s.to_string()).collect()
    });
    fn ns_priority(name: &str, front: bool) -> i32 {
        if front {
            if name == "xmlns" { return 3; }
            if name.starts_with("xmlns:") { return 2; }
        }
        if name.contains(':') { return 1; }
        0
    }
    fn walk(children: &mut [XastChild], order: &[String], front: bool) {
        for c in children.iter_mut() {
            if let XastChild::Element(el) = c {
                el.attributes.sort_by(|a, b| {
                    let pa = ns_priority(&a.0, front);
                    let pb = ns_priority(&b.0, front);
                    if pa != pb { return pb.cmp(&pa); }
                    let a_part = a.0.split('-').next().unwrap_or("");
                    let b_part = b.0.split('-').next().unwrap_or("");
                    if a_part != b_part {
                        let ai = order.iter().position(|x| x == a_part);
                        let bi = order.iter().position(|x| x == b_part);
                        match (ai, bi) {
                            (Some(x), Some(y)) => if x != y { return x.cmp(&y); },
                            (Some(_), None) => return std::cmp::Ordering::Less,
                            (None, Some(_)) => return std::cmp::Ordering::Greater,
                            (None, None) => {}
                        }
                    }
                    if a.0 < b.0 { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater }
                });
                walk(&mut el.children, order, front);
            }
        }
    }
    walk(&mut root.children, &order, xmlns_front);
}

// ---------- sortDefsChildren ----------

pub fn sort_defs_children(root: &mut XastRoot) {
    fn walk(children: &mut [XastChild]) {
        for c in children.iter_mut() {
            if let XastChild::Element(el) = c {
                if el.name == "defs" {
                    let mut freq: HashMap<String, usize> = HashMap::new();
                    for ch in &el.children {
                        if let XastChild::Element(e) = ch {
                            *freq.entry(e.name.clone()).or_insert(0) += 1;
                        }
                    }
                    el.children.sort_by(|a, b| {
                        let (XastChild::Element(ae), XastChild::Element(be)) = (a, b) else {
                            return std::cmp::Ordering::Equal;
                        };
                        let fa = freq.get(&ae.name).copied().unwrap_or(0);
                        let fb = freq.get(&be.name).copied().unwrap_or(0);
                        if fa != fb { return fb.cmp(&fa); }
                        if ae.name.len() != be.name.len() { return be.name.len().cmp(&ae.name.len()); }
                        if ae.name != be.name {
                            return if ae.name > be.name { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater };
                        }
                        std::cmp::Ordering::Equal
                    });
                }
                walk(&mut el.children);
            }
        }
    }
    walk(&mut root.children);
}
