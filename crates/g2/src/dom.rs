//! XAST-like DOM, SAX-style parser and pretty stringifier.
//! Mirrors `lib/parser.js` (text trimmed except in text elements, comments
//! trimmed) and `lib/stringifier.js` with `pretty: true`, indent 4.

use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_UID: AtomicU64 = AtomicU64::new(1);
pub fn next_uid() -> u64 {
    NEXT_UID.fetch_add(1, Ordering::Relaxed)
}

pub fn is_text_elem(name: &str) -> bool {
    matches!(
        name,
        "a" | "altGlyph"
            | "altGlyphDef"
            | "altGlyphItem"
            | "glyph"
            | "glyphRef"
            | "text"
            | "textPath"
            | "tref"
            | "tspan"
            | "pre"
            | "title"
    )
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Element(Element),
    Text(String),
    Cdata(String),
    Comment(String),
    Doctype(String),
    Instruction { name: String, value: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub uid: u64,
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Element {
    pub fn new(name: &str) -> Self {
        Element { uid: next_uid(), name: name.to_string(), attrs: Vec::new(), children: Vec::new() }
    }
    pub fn get(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
    pub fn has(&self, name: &str) -> bool {
        self.attrs.iter().any(|(k, _)| k == name)
    }
    pub fn set(&mut self, name: &str, value: &str) {
        if let Some(slot) = self.attrs.iter_mut().find(|(k, _)| k == name) {
            slot.1 = value.to_string();
        } else {
            self.attrs.push((name.to_string(), value.to_string()));
        }
    }
    pub fn remove_attr(&mut self, name: &str) {
        self.attrs.retain(|(k, _)| k != name);
    }
    /// Concatenated text/cdata child values (style extraction).
    pub fn text_content(&self) -> String {
        let mut s = String::new();
        for c in &self.children {
            match c {
                Node::Text(v) | Node::Cdata(v) => s.push_str(v),
                _ => {}
            }
        }
        s
    }
}

// ---------- traversal helpers ----------
pub fn collect_element_uids(nodes: &[Node], out: &mut Vec<u64>) {
    for n in nodes {
        if let Node::Element(e) = n {
            out.push(e.uid);
            collect_element_uids(&e.children, out);
        }
    }
}

pub fn find_path(nodes: &[Node], uid: u64) -> Option<Vec<usize>> {
    for (i, n) in nodes.iter().enumerate() {
        if let Node::Element(e) = n {
            if e.uid == uid {
                return Some(vec![i]);
            }
            if let Some(mut sub) = find_path(&e.children, uid) {
                let mut p = vec![i];
                p.append(&mut sub);
                return Some(p);
            }
        }
    }
    None
}

pub fn get_element<'a>(nodes: &'a [Node], path: &[usize]) -> Option<&'a Element> {
    let mut cur = nodes;
    let mut result = None;
    for &i in path {
        match cur.get(i) {
            Some(Node::Element(e)) => {
                result = Some(e);
                cur = &e.children;
            }
            _ => return None,
        }
    }
    result
}

pub fn get_element_mut<'a>(nodes: &'a mut [Node], path: &[usize]) -> Option<&'a mut Element> {
    if path.is_empty() {
        return None;
    }
    let mut cur = nodes;
    for (idx, &i) in path.iter().enumerate() {
        match cur.get_mut(i) {
            Some(Node::Element(e)) => {
                if idx == path.len() - 1 {
                    return Some(e);
                }
                cur = &mut e.children;
            }
            _ => return None,
        }
    }
    None
}

pub fn element_by_uid<'a>(nodes: &'a [Node], uid: u64) -> Option<&'a Element> {
    let path = find_path(nodes, uid)?;
    get_element(nodes, &path)
}
pub fn element_by_uid_mut<'a>(nodes: &'a mut [Node], uid: u64) -> Option<&'a mut Element> {
    let path = find_path(nodes, uid)?;
    get_element_mut(nodes, &path)
}

/// Detach the element with `uid` from its parent. Returns true if removed.
pub fn detach_by_uid(nodes: &mut Vec<Node>, uid: u64) -> bool {
    if let Some(pos) = nodes.iter().position(|n| matches!(n, Node::Element(e) if e.uid == uid)) {
        nodes.remove(pos);
        return true;
    }
    for n in nodes.iter_mut() {
        if let Node::Element(e) = n {
            if detach_by_uid(&mut e.children, uid) {
                return true;
            }
        }
    }
    false
}

// ---------- entity handling ----------
pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(idx) = rest.find('&') {
        out.push_str(&rest[..idx]);
        let after = &rest[idx + 1..];
        match after.find(';') {
            Some(end) if end <= 10 => {
                let ent = &after[..end];
                let decoded = match ent {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    _ => {
                        if let Some(hex) = ent.strip_prefix("#x").or_else(|| ent.strip_prefix("#X")) {
                            u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
                        } else if let Some(dec) = ent.strip_prefix('#') {
                            dec.parse::<u32>().ok().and_then(char::from_u32)
                        } else {
                            None
                        }
                    }
                };
                match decoded {
                    Some(c) => {
                        out.push(c);
                        rest = &after[end + 1..];
                    }
                    None => {
                        out.push('&');
                        rest = after;
                    }
                }
            }
            _ => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn encode_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '\'' => out.push_str("&apos;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}
fn encode_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

// ---------- parser ----------
struct Scanner {
    chars: Vec<char>,
    pos: usize,
}
impl Scanner {
    fn starts_with(&self, s: &str) -> bool {
        s.chars().enumerate().all(|(i, c)| self.chars.get(self.pos + i) == Some(&c))
    }
    fn take_until(&mut self, end: &str) -> String {
        let end_chars: Vec<char> = end.chars().collect();
        let start = self.pos;
        let mut i = self.pos;
        while i + end_chars.len() <= self.chars.len() {
            if self.chars[i..i + end_chars.len()] == end_chars[..] {
                let s: String = self.chars[start..i].iter().collect();
                self.pos = i + end_chars.len();
                return s;
            }
            i += 1;
        }
        let s: String = self.chars[start..].iter().collect();
        self.pos = self.chars.len();
        s
    }
    fn take_until_char(&mut self, stop: &[char]) -> String {
        let start = self.pos;
        while self.pos < self.chars.len() && !stop.contains(&self.chars[self.pos]) {
            self.pos += 1;
        }
        self.chars[start..self.pos].iter().collect()
    }
}

pub fn parse(svg: &str) -> Vec<Node> {
    let mut sc = Scanner { chars: svg.chars().collect(), pos: 0 };
    parse_nodes(&mut sc, None)
}

fn parse_nodes(sc: &mut Scanner, parent_name: Option<&str>) -> Vec<Node> {
    let mut nodes = Vec::new();
    while sc.pos < sc.chars.len() {
        if sc.starts_with("</") {
            break;
        } else if sc.starts_with("<!--") {
            sc.pos += 4;
            let v = sc.take_until("-->");
            nodes.push(Node::Comment(v.trim().to_string()));
        } else if sc.starts_with("<![CDATA[") {
            sc.pos += 9;
            let v = sc.take_until("]]>");
            nodes.push(Node::Cdata(v));
        } else if sc.starts_with("<!DOCTYPE") || sc.starts_with("<!doctype") {
            sc.pos += 9;
            let v = sc.take_until(">");
            nodes.push(Node::Doctype(v));
        } else if sc.starts_with("<!") {
            sc.pos += 2;
            let v = sc.take_until(">");
            nodes.push(Node::Doctype(format!(" {v}")));
        } else if sc.starts_with("<?") {
            sc.pos += 2;
            let v = sc.take_until("?>");
            let v = v.trim().to_string();
            let (name, value) = match v.find(char::is_whitespace) {
                Some(i) => (v[..i].to_string(), v[i..].trim().to_string()),
                None => (v, String::new()),
            };
            nodes.push(Node::Instruction { name, value });
        } else if sc.starts_with("<") {
            // element
            sc.pos += 1;
            let name = sc.take_until_char(&[' ', '\t', '\r', '\n', '/', '>']);
            let mut elem = Element::new(&name);
            let mut self_closing = false;
            loop {
                // skip whitespace
                while sc.pos < sc.chars.len() && sc.chars[sc.pos].is_whitespace() {
                    sc.pos += 1;
                }
                if sc.starts_with("/>") {
                    sc.pos += 2;
                    self_closing = true;
                    break;
                }
                if sc.starts_with(">") {
                    sc.pos += 1;
                    break;
                }
                if sc.pos >= sc.chars.len() {
                    break;
                }
                let aname = sc.take_until_char(&['=', ' ', '\t', '\r', '\n', '/', '>']);
                if aname.is_empty() {
                    sc.pos += 1;
                    continue;
                }
                while sc.pos < sc.chars.len() && sc.chars[sc.pos].is_whitespace() {
                    sc.pos += 1;
                }
                let mut avalue = String::new();
                if sc.pos < sc.chars.len() && sc.chars[sc.pos] == '=' {
                    sc.pos += 1;
                    while sc.pos < sc.chars.len() && sc.chars[sc.pos].is_whitespace() {
                        sc.pos += 1;
                    }
                    if sc.pos < sc.chars.len() && (sc.chars[sc.pos] == '"' || sc.chars[sc.pos] == '\'') {
                        let q = sc.chars[sc.pos];
                        sc.pos += 1;
                        let start = sc.pos;
                        while sc.pos < sc.chars.len() && sc.chars[sc.pos] != q {
                            sc.pos += 1;
                        }
                        avalue = sc.chars[start..sc.pos].iter().collect();
                        if sc.pos < sc.chars.len() {
                            sc.pos += 1;
                        }
                    } else {
                        avalue = sc.take_until_char(&[' ', '\t', '\r', '\n', '/', '>']);
                    }
                }
                elem.attrs.push((aname, decode_entities(&avalue)));
            }
            if !self_closing {
                let children = parse_nodes(sc, Some(&name));
                // consume closing tag
                if sc.starts_with("</") {
                    sc.pos += 2;
                    let _ = sc.take_until(">");
                }
                elem.children = children;
            }
            nodes.push(Node::Element(elem));
        } else {
            let raw = sc.take_until_char(&['<']);
            let decoded = decode_entities(&raw);
            match parent_name {
                Some(p) if is_text_elem(p) => {
                    if !decoded.is_empty() {
                        nodes.push(Node::Text(decoded));
                    }
                }
                Some(_) => {
                    let v = decoded.trim().to_string();
                    if !v.is_empty() {
                        nodes.push(Node::Text(v));
                    }
                }
                None => {}
            }
        }
    }
    nodes
}

// ---------- pretty stringifier (js2svg pretty:true, indent 4) ----------
pub fn stringify_pretty(nodes: &[Node]) -> String {
    let mut out = String::new();
    stringify_nodes(nodes, 1, false, &mut out);
    out
}

fn indent(level: usize) -> String {
    "    ".repeat(level.saturating_sub(1))
}

fn stringify_nodes(nodes: &[Node], level: usize, in_text: bool, out: &mut String) {
    for n in nodes {
        match n {
            Node::Doctype(v) => {
                out.push_str("<!DOCTYPE");
                out.push_str(v);
                out.push_str(">\n");
            }
            Node::Instruction { name, value } => {
                out.push_str("<?");
                out.push_str(name);
                if !value.is_empty() {
                    out.push(' ');
                    out.push_str(value);
                } else {
                    // JS always emits `name + ' ' + value`
                    out.push(' ');
                }
                out.push_str("?>\n");
            }
            Node::Comment(v) => {
                out.push_str("<!--");
                out.push_str(v);
                out.push_str("-->\n");
            }
            Node::Cdata(v) => {
                if !in_text {
                    out.push_str(&indent(level));
                }
                out.push_str("<![CDATA[");
                out.push_str(v);
                out.push_str("]]>\n");
            }
            Node::Text(v) => {
                if !in_text {
                    out.push_str(&indent(level));
                }
                out.push_str(&encode_text(v));
                if !in_text {
                    out.push('\n');
                }
            }
            Node::Element(e) => stringify_element(e, level, in_text, out),
        }
    }
}

fn stringify_attrs(e: &Element) -> String {
    let mut s = String::new();
    for (k, v) in &e.attrs {
        s.push(' ');
        s.push_str(k);
        s.push_str("=\"");
        s.push_str(&encode_attr(v));
        s.push('"');
    }
    s
}

fn stringify_element(e: &Element, level: usize, in_text: bool, out: &mut String) {
    if e.children.is_empty() {
        if !in_text {
            out.push_str(&indent(level));
        }
        out.push('<');
        out.push_str(&e.name);
        out.push_str(&stringify_attrs(e));
        if in_text {
            out.push_str("/>");
        } else {
            out.push_str("/>\n");
        }
        return;
    }
    if in_text {
        out.push('<');
        out.push_str(&e.name);
        out.push_str(&stringify_attrs(e));
        out.push('>');
        stringify_nodes(&e.children, level + 1, true, out);
        out.push_str("</");
        out.push_str(&e.name);
        out.push('>');
        return;
    }
    if is_text_elem(&e.name) {
        out.push_str(&indent(level));
        out.push('<');
        out.push_str(&e.name);
        out.push_str(&stringify_attrs(e));
        out.push('>');
        stringify_nodes(&e.children, level + 1, true, out);
        out.push_str("</");
        out.push_str(&e.name);
        out.push_str(">\n");
        return;
    }
    out.push_str(&indent(level));
    out.push('<');
    out.push_str(&e.name);
    out.push_str(&stringify_attrs(e));
    out.push_str(">\n");
    stringify_nodes(&e.children, level + 1, false, out);
    out.push_str(&indent(level));
    out.push_str("</");
    out.push_str(&e.name);
    out.push_str(">\n");
}
