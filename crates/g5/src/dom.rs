#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Element(Element),
    Text(String),
    Comment(String),
    CData(String),
    Doctype(String),
    Instruction(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Element {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
    pub fn has(&self, name: &str) -> bool {
        self.attrs.iter().any(|(k, _)| k == name)
    }
    pub fn set(&mut self, name: &str, value: String) {
        if let Some(slot) = self.attrs.iter_mut().find(|(k, _)| k == name) {
            slot.1 = value;
        } else {
            self.attrs.push((name.to_string(), value));
        }
    }
    pub fn remove_attr(&mut self, name: &str) {
        self.attrs.retain(|(k, _)| k != name);
    }
}

#[derive(Clone, Debug, Default)]
pub struct Document {
    pub children: Vec<Node>,
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(idx) = rest.find('&') {
        out.push_str(&rest[..idx]);
        let after = &rest[idx + 1..];
        if let Some(end) = after.find(';') {
            let ent = &after[..end];
            let decoded = match ent {
                "amp" => Some("&".to_string()),
                "lt" => Some("<".to_string()),
                "gt" => Some(">".to_string()),
                "quot" => Some("\"".to_string()),
                "apos" => Some("'".to_string()),
                _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                    u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32).map(|c| c.to_string())
                }
                _ if ent.starts_with('#') => {
                    ent[1..].parse::<u32>().ok().and_then(char::from_u32).map(|c| c.to_string())
                }
                _ => None,
            };
            if let Some(d) = decoded {
                out.push_str(&d);
                rest = &after[end + 1..];
                continue;
            }
        }
        out.push('&');
        rest = after;
    }
    out.push_str(rest);
    out
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('"', "&quot;")
}
fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;")
}

pub fn parse(input: &str) -> Document {
    let chars: Vec<char> = input.chars().collect();
    let mut pos = 0usize;
    let children = parse_nodes(&chars, &mut pos, None);
    Document { children }
}

fn text_of(chars: &[char], a: usize, b: usize) -> String {
    chars[a..b].iter().collect()
}

fn parse_nodes(chars: &[char], pos: &mut usize, closing: Option<&str>) -> Vec<Node> {
    let mut nodes = Vec::new();
    while *pos < chars.len() {
        if chars[*pos] == '<' {
            if starts_with(chars, *pos, "</") {
                let end = find_char(chars, *pos, '>');
                let name = text_of(chars, *pos + 2, end.unwrap_or(chars.len())).trim().to_string();
                *pos = end.map(|e| e + 1).unwrap_or(chars.len());
                if closing.map(|c| c == name).unwrap_or(false) || closing.is_some() {
                    return nodes;
                }
                continue;
            } else if starts_with(chars, *pos, "<!--") {
                let end = find_str(chars, *pos, "-->");
                let inner_end = end.unwrap_or(chars.len());
                nodes.push(Node::Comment(text_of(chars, *pos + 4, inner_end)));
                *pos = end.map(|e| e + 3).unwrap_or(chars.len());
            } else if starts_with(chars, *pos, "<![CDATA[") {
                let end = find_str(chars, *pos, "]]>");
                let inner_end = end.unwrap_or(chars.len());
                nodes.push(Node::CData(text_of(chars, *pos + 9, inner_end)));
                *pos = end.map(|e| e + 3).unwrap_or(chars.len());
            } else if starts_with(chars, *pos, "<!") {
                let end = find_char(chars, *pos, '>');
                let e = end.unwrap_or(chars.len());
                nodes.push(Node::Doctype(text_of(chars, *pos, e + 1)));
                *pos = e + 1;
            } else if starts_with(chars, *pos, "<?") {
                let end = find_str(chars, *pos, "?>");
                let e = end.map(|x| x + 2).unwrap_or(chars.len());
                nodes.push(Node::Instruction(text_of(chars, *pos, e)));
                *pos = e;
            } else {
                // element
                let el = parse_element(chars, pos);
                nodes.push(Node::Element(el));
            }
        } else {
            let start = *pos;
            while *pos < chars.len() && chars[*pos] != '<' {
                *pos += 1;
            }
            let raw = text_of(chars, start, *pos);
            if !raw.is_empty() {
                nodes.push(Node::Text(decode_entities(&raw)));
            }
        }
    }
    nodes
}

fn starts_with(chars: &[char], pos: usize, pat: &str) -> bool {
    pat.chars().enumerate().all(|(i, c)| pos + i < chars.len() && chars[pos + i] == c)
}
fn find_char(chars: &[char], pos: usize, c: char) -> Option<usize> {
    (pos..chars.len()).find(|&i| chars[i] == c)
}
fn find_str(chars: &[char], pos: usize, pat: &str) -> Option<usize> {
    let p: Vec<char> = pat.chars().collect();
    (pos..=chars.len().saturating_sub(p.len())).find(|&i| chars[i..i + p.len()] == p[..])
}

fn parse_element(chars: &[char], pos: &mut usize) -> Element {
    *pos += 1; // <
    let start = *pos;
    while *pos < chars.len() && !chars[*pos].is_whitespace() && chars[*pos] != '>' && chars[*pos] != '/' {
        *pos += 1;
    }
    let name = text_of(chars, start, *pos);
    let mut attrs = Vec::new();
    let mut self_closing = false;
    loop {
        while *pos < chars.len() && chars[*pos].is_whitespace() { *pos += 1; }
        if *pos >= chars.len() { break; }
        if starts_with(chars, *pos, "/>") {
            *pos += 2;
            self_closing = true;
            break;
        }
        if chars[*pos] == '>' {
            *pos += 1;
            break;
        }
        // attr name
        let astart = *pos;
        while *pos < chars.len() && !chars[*pos].is_whitespace() && chars[*pos] != '=' && chars[*pos] != '>' && chars[*pos] != '/' {
            *pos += 1;
        }
        let aname = text_of(chars, astart, *pos);
        while *pos < chars.len() && chars[*pos].is_whitespace() { *pos += 1; }
        let mut avalue = String::new();
        if *pos < chars.len() && chars[*pos] == '=' {
            *pos += 1;
            while *pos < chars.len() && chars[*pos].is_whitespace() { *pos += 1; }
            if *pos < chars.len() && (chars[*pos] == '"' || chars[*pos] == '\'') {
                let q = chars[*pos];
                *pos += 1;
                let vstart = *pos;
                while *pos < chars.len() && chars[*pos] != q { *pos += 1; }
                avalue = decode_entities(&text_of(chars, vstart, *pos));
                *pos += 1;
            }
        }
        if !aname.is_empty() {
            attrs.push((aname, avalue));
        }
    }
    if self_closing {
        return Element { name, attrs, children: Vec::new() };
    }
    let children = parse_nodes(chars, pos, Some(&name));
    Element { name, attrs, children }
}

/// Pretty serialization matching SVGO `js2svg: { pretty: true }` (4-space indent).
pub fn stringify(doc: &Document) -> String {
    let mut out = String::new();
    for node in &doc.children {
        stringify_node(node, 0, &mut out);
    }
    out.trim_end_matches('\n').to_string()
}

fn indent(depth: usize) -> String {
    "    ".repeat(depth)
}

fn stringify_node(node: &Node, depth: usize, out: &mut String) {
    match node {
        Node::Text(t) => {
            let trimmed = t.trim();
            if !trimmed.is_empty() {
                out.push_str(&indent(depth));
                out.push_str(&escape_text(trimmed));
                out.push('\n');
            }
        }
        Node::Comment(c) => {
            out.push_str(&indent(depth));
            out.push_str("<!--");
            out.push_str(c);
            out.push_str("-->\n");
        }
        Node::CData(c) => {
            out.push_str(&indent(depth));
            out.push_str("<![CDATA[");
            out.push_str(c);
            out.push_str("]]>\n");
        }
        Node::Doctype(d) | Node::Instruction(d) => {
            out.push_str(&indent(depth));
            out.push_str(d);
            out.push('\n');
        }
        Node::Element(el) => {
            let ind = indent(depth);
            let attrs = attrs_string(el);
            if TEXT_ELEMS.contains(&el.name.as_str()) && !el.children.is_empty() {
                out.push_str(&format!("{}<{}{}>", ind, el.name, attrs));
                for c in &el.children {
                    stringify_inline(c, out);
                }
                out.push_str(&format!("</{}>\n", el.name));
                return;
            }
            // significant children: drop whitespace-only text
            let children: Vec<&Node> = el.children.iter().filter(|n| {
                !matches!(n, Node::Text(t) if t.trim().is_empty())
            }).collect();
            if children.is_empty() {
                out.push_str(&format!("{}<{}{}/>\n", ind, el.name, attrs));
            } else if children.len() == 1 {
                if let Node::Text(t) = children[0] {
                    // Preserve internal newlines: first line re-indented, rest verbatim.
                    let trimmed = t.trim();
                    out.push_str(&format!("{}<{}{}>\n", ind, el.name, attrs));
                    out.push_str(&indent(depth + 1));
                    // text that is a single word/line stays inline in SVGO for non-style?
                    // SVGO pretty prints text inline when... observed: <text>uwu</text> inline,
                    // <style> content on its own lines. Distinguish by element name.
                    if el.name == "style" || trimmed.contains('\n') {
                        out.push_str(&escape_text(trimmed));
                        out.push('\n');
                        out.push_str(&format!("{}</{}>\n", ind, el.name));
                    } else {
                        // inline: rebuild without the newlines added above
                        out.truncate(out.len() - (indent(depth + 1).len() + 1));
                        out.push_str(&escape_text(trimmed));
                        out.push_str(&format!("</{}>\n", el.name));
                    }
                } else {
                    out.push_str(&format!("{}<{}{}>\n", ind, el.name, attrs));
                    for c in children {
                        stringify_node(c, depth + 1, out);
                    }
                    out.push_str(&format!("{}</{}>\n", ind, el.name));
                }
            } else {
                out.push_str(&format!("{}<{}{}>\n", ind, el.name, attrs));
                for c in children {
                    stringify_node(c, depth + 1, out);
                }
                out.push_str(&format!("{}</{}>\n", ind, el.name));
            }
        }
    }
}

const TEXT_ELEMS: &[&str] = &[
    "a", "altGlyph", "altGlyphDef", "altGlyphItem", "glyph", "glyphRef", "text", "textPath",
    "tref", "tspan", "pre", "title",
];

pub fn is_text_elem(name: &str) -> bool {
    TEXT_ELEMS.contains(&name)
}

fn stringify_inline(node: &Node, out: &mut String) {
    match node {
        Node::Text(t) => out.push_str(&escape_text(t)),
        Node::Comment(c) => {
            out.push_str("<!--");
            out.push_str(c);
            out.push_str("-->");
        }
        Node::CData(c) => {
            out.push_str("<![CDATA[");
            out.push_str(c);
            out.push_str("]]>");
        }
        Node::Doctype(d) | Node::Instruction(d) => out.push_str(d),
        Node::Element(el) => {
            let attrs = attrs_string(el);
            if el.children.is_empty() {
                out.push_str(&format!("<{}{}/>", el.name, attrs));
            } else {
                out.push_str(&format!("<{}{}>", el.name, attrs));
                for c in &el.children {
                    stringify_inline(c, out);
                }
                out.push_str(&format!("</{}>", el.name));
            }
        }
    }
}

fn attrs_string(el: &Element) -> String {
    let mut s = String::new();
    for (k, v) in &el.attrs {
        s.push_str(&format!(" {}=\"{}\"", k, escape_attr(v)));
    }
    s
}

/// Collect all elements (mutable walk is done via recursive helpers in plugins).
pub fn collect_style_texts(nodes: &[Node], out: &mut Vec<String>) {
    for n in nodes {
        if let Node::Element(el) = n {
            if el.name == "style" {
                let mut txt = String::new();
                for c in &el.children {
                    if let Node::Text(t) = c { txt.push_str(t); }
                    if let Node::CData(t) = c { txt.push_str(t); }
                }
                out.push(txt);
            }
            collect_style_texts(&el.children, out);
        }
    }
}
