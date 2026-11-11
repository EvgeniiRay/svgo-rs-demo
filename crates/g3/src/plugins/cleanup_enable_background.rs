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

/// Split on `;` at paren depth 0, respecting strings and escapes
/// (css-tree declarationList parsing; `;` inside `url('a;b')` is data).
fn split_decls(style: &str) -> Vec<String> {
    let chars: Vec<char> = style.chars().collect();
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match quote {
            Some(q) => {
                cur.push(c);
                if c == '\\' && i + 1 < chars.len() {
                    cur.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if c == q {
                    quote = None;
                }
                i += 1;
            }
            None => match c {
                '"' | '\'' => {
                    quote = Some(c);
                    cur.push(c);
                    i += 1;
                }
                '(' | '[' => {
                    depth += 1;
                    cur.push(c);
                    i += 1;
                }
                ')' | ']' => {
                    depth -= 1;
                    cur.push(c);
                    i += 1;
                }
                ';' if depth == 0 => {
                    parts.push(cur.clone());
                    cur.clear();
                    i += 1;
                }
                _ => {
                    cur.push(c);
                    i += 1;
                }
            },
        }
    }
    parts.push(cur);
    parts
}

fn parse_style(style: &str) -> Vec<Decl> {
    split_decls(style)
        .iter()
        .filter(|s| !s.trim().is_empty())
        .map(|chunk| {
            // first ':' at paren depth 0 outside strings
            let chars: Vec<char> = chunk.chars().collect();
            let mut colon = None;
            let mut depth = 0i32;
            let mut quote: Option<char> = None;
            let mut i = 0;
            while i < chars.len() {
                let c = chars[i];
                match quote {
                    Some(q) => {
                        if c == '\\' {
                            i += 2;
                            continue;
                        }
                        if c == q {
                            quote = None;
                        }
                    }
                    None => match c {
                        '"' | '\'' => quote = Some(c),
                        '(' | '[' => depth += 1,
                        ')' | ']' => depth -= 1,
                        ':' if depth == 0 => {
                            colon = Some(chunk.char_indices().nth(i).map(|(b, _)| b).unwrap_or(0));
                            break;
                        }
                        _ => {}
                    },
                }
                i += 1;
            }
            match colon {
                Some(ci) => Decl {
                    prop: chunk[..ci].trim().to_string(),
                    value: chunk[ci + 1..].trim().to_string(),
                    raw: None,
                },
                None => Decl {
                    prop: String::new(),
                    value: String::new(),
                    raw: Some(chunk.trim().to_string()),
                },
            }
        })
        .collect()
}

/// Decode one CSS escape starting at the `\` (chars[i]); returns decoded
/// text and the index just past the escape (css-tree decodeEscaped).
fn decode_escape(chars: &[char], i: usize) -> (String, usize) {
    if i + 1 >= chars.len() {
        return (String::new(), chars.len());
    }
    let n = chars[i + 1];
    if n == '\n' || n == '\r' || n == '\u{c}' {
        let mut j = i + 2;
        if n == '\r' && j < chars.len() && chars[j] == '\n' {
            j += 1;
        }
        return (String::new(), j);
    }
    if n.is_ascii_hexdigit() {
        let mut hex = String::new();
        let mut j = i + 1;
        while j < chars.len() && hex.len() < 6 && chars[j].is_ascii_hexdigit() {
            hex.push(chars[j]);
            j += 1;
        }
        if j < chars.len() && chars[j].is_whitespace() {
            if chars[j] == '\r' && j + 1 < chars.len() && chars[j + 1] == '\n' {
                j += 1;
            }
            j += 1;
        }
        let v = u32::from_str_radix(&hex, 16).unwrap_or(0);
        let ch = if v == 0 || v > 0x10FFFF || (0xD800..0xE000).contains(&v) {
            '\u{FFFD}'
        } else {
            char::from_u32(v).unwrap_or('\u{FFFD}')
        };
        return (ch.to_string(), j);
    }
    (n.to_string(), i + 2)
}

fn parse_css_string(chars: &[char], start: usize) -> (String, usize) {
    // chars[start] is the quote; returns decoded content and index past it
    let q = chars[start];
    let mut out = String::new();
    let mut i = start + 1;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            let (d, ni) = decode_escape(chars, i);
            out.push_str(&d);
            i = ni;
        } else if c == q {
            return (out, i + 1);
        } else {
            out.push(c);
            i += 1;
        }
    }
    (out, i)
}

fn encode_url(decoded: &str) -> String {
    // css-tree utils/url.encode: unquoted, escaping special chars
    let mut out = String::from("url(");
    let mut hex_pending = false;
    for c in decoded.chars() {
        let code = c as u32;
        if code == 0 {
            out.push_str("\\uFFFD");
            hex_pending = false;
        } else if code <= 0x1f || code == 0x7f {
            out.push_str(&format!("\\{code:x}"));
            hex_pending = true;
        } else if matches!(c, ' ' | '\\' | '"' | '\'' | '(' | ')') {
            out.push('\\');
            out.push(c);
            hex_pending = false;
        } else {
            if hex_pending && c.is_ascii_hexdigit() {
                out.push(' ');
            }
            out.push(c);
            hex_pending = false;
        }
    }
    out.push(')');
    out
}

fn encode_string(decoded: &str) -> String {
    // css-tree utils/string.encode (double quotes)
    let mut out = String::from("\"");
    let mut hex_pending = false;
    for c in decoded.chars() {
        let code = c as u32;
        if code == 0 {
            out.push_str("\\uFFFD");
            hex_pending = false;
        } else if code <= 0x1f || code == 0x7f {
            out.push_str(&format!("\\{code:x}"));
            hex_pending = true;
        } else if c == '"' || c == '\\' {
            out.push('\\');
            out.push(c);
            hex_pending = false;
        } else {
            if hex_pending && (c.is_ascii_hexdigit() || c.is_whitespace()) {
                out.push(' ');
            }
            out.push(c);
            hex_pending = false;
        }
    }
    out.push('"');
    out
}

/// Re-generate a declaration value the way `csstree.generate` does for the
/// shapes style attributes carry: `url(...)` is emitted unquoted with
/// escapes, strings use double quotes, no space follows a comma.
fn csstree_value(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        // url( ... )
        if (chars[i] == 'u' || chars[i] == 'U')
            && chars[i..]
                .iter()
                .collect::<String>()
                .to_lowercase()
                .starts_with("url(")
        {
            let mut j = i + 4;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            let decoded;
            if j < chars.len() && (chars[j] == '"' || chars[j] == '\'') {
                let (d, ni) = parse_css_string(&chars, j);
                decoded = d;
                j = ni;
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                if j < chars.len() && chars[j] == ')' {
                    j += 1;
                }
            } else {
                let mut d = String::new();
                while j < chars.len() && chars[j] != ')' {
                    if chars[j] == '\\' {
                        let (dec, ni) = decode_escape(&chars, j);
                        d.push_str(&dec);
                        j = ni;
                    } else {
                        d.push(chars[j]);
                        j += 1;
                    }
                }
                if j < chars.len() {
                    j += 1; // ')'
                }
                decoded = d.trim().to_string();
            }
            out.push_str(&encode_url(&decoded));
            i = j;
        } else if chars[i] == '"' || chars[i] == '\'' {
            let (d, ni) = parse_css_string(&chars, i);
            out.push_str(&encode_string(&d));
            i = ni;
        } else if chars[i].is_whitespace() {
            // collapse runs; drop after ','/'(' (handled via out) and before ')'/','
            let mut j = i;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            let next = chars.get(j).copied();
            if !out.is_empty()
                && !matches!(out.chars().last(), Some(',') | Some('('))
                && !matches!(next, Some(')') | Some(','))
                && next.is_some()
            {
                out.push(' ');
            }
            i = j;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    let v = out.trim().to_string();
    // csstree emits `!important` glued to the value
    if let Some(pos) = v.to_lowercase().rfind(" !important") {
        if v[pos + 11..].trim().is_empty() {
            return format!("{}!important", &v[..pos]);
        }
    }
    v
}

fn generate_style(decls: &[Decl]) -> String {
    let parts: Vec<String> = decls
        .iter()
        .map(|d| match &d.raw {
            Some(r) => r.clone(),
            None => format!("{}:{}", d.prop, csstree_value(&d.value)),
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
        Some(r) if r.chars().next().map(|c| c.is_whitespace()).unwrap_or(false) => r.trim_start(),
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
