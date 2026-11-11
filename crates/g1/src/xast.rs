//! XAST DOM, parser and pretty stringifier.
//!
//! Node kinds mirror SVGO's `lib/types.ts` Xast types: root, element,
//! text, comment, cdata, doctype and instruction (processing instruction).
//! Entities in text and attribute values are preserved verbatim
//! (e.g. `&amp;`), matching SVGO fixtures.

#[derive(Debug, Clone, PartialEq)]
pub enum XastChild {
    Element(XastElement),
    Text(String),
    Comment(String),
    Cdata(String),
    /// Verbatim contents between `<!DOCTYPE` and `>`.
    Doctype(String),
    /// Processing instruction: `<?name value?>`.
    Instruction { name: String, value: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct XastElement {
    pub name: String,
    /// Attributes in document order.
    pub attributes: Vec<(String, String)>,
    pub children: Vec<XastChild>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct XastRoot {
    pub children: Vec<XastChild>,
}

/// Equivalent of JS `detachNodeFromParent`: remove `child` from a child list.
pub fn detach_child(children: &mut Vec<XastChild>, child: &XastChild) {
    children.retain(|c| c != child);
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    fn starts_with(&self, s: &str) -> bool {
        s.chars().enumerate().all(|(i, c)| self.chars.get(self.pos + i) == Some(&c))
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }
    fn expect(&mut self, s: &str) -> Result<(), String> {
        if self.starts_with(s) {
            self.pos += s.chars().count();
            Ok(())
        } else {
            Err(format!("parse error at {}: expected {:?}", self.pos, s))
        }
    }
    fn take_until(&mut self, end: &str) -> String {
        let mut out = String::new();
        while self.peek().is_some() && !self.starts_with(end) {
            out.push(self.bump().unwrap());
        }
        out
    }
}

/// Parse SVG/XML markup into an [`XastRoot`].
pub fn parse(input: &str) -> Result<XastRoot, String> {
    let mut p = Parser { chars: input.chars().collect(), pos: 0 };
    let children = parse_children(&mut p, None)?;
    Ok(XastRoot { children })
}

/// Mirror of JS `textElems` (`plugins/_collections.js`): text inside
/// these elements is preserved verbatim by the parser/stringifier.
fn is_text_elem(name: &str) -> bool {
    matches!(
        name,
        "a" | "altGlyph" | "altGlyphDef" | "altGlyphItem" | "glyph" | "glyphRef" | "text"
            | "textPath" | "tref" | "tspan" | "pre" | "title"
    )
}

fn parse_children(p: &mut Parser, closing: Option<&str>) -> Result<Vec<XastChild>, String> {
    let mut children = Vec::new();
    loop {
        match p.peek() {
            None => {
                if closing.is_some() {
                    return Err("parse error: unexpected end of input".into());
                }
                return Ok(children);
            }
            Some('<') => {
                if p.starts_with("</") {
                    p.pos += 2;
                    let name = p.take_until(">");
                    p.expect(">")?;
                    if closing == Some(name.trim()) {
                        return Ok(children);
                    }
                    return Err(format!("parse error: unexpected closing tag {name}"));
                } else if p.starts_with("<!--") {
                    p.pos += 4;
                    let value = p.take_until("-->");
                    p.expect("-->")?;
                    children.push(XastChild::Comment(value));
                } else if p.starts_with("<![CDATA[") {
                    p.pos += 9;
                    let value = p.take_until("]]>");
                    p.expect("]]>")?;
                    children.push(XastChild::Cdata(value));
                } else if p.starts_with("<!DOCTYPE") || p.starts_with("<!doctype") {
                    children.push(XastChild::Doctype(parse_doctype(p)?));
                } else if p.starts_with("<?") {
                    p.pos += 2;
                    let raw = p.take_until("?>");
                    p.expect("?>")?;
                    let (name, value) = match raw.split_once(char::is_whitespace) {
                        Some((n, v)) => (n.to_string(), v.trim().to_string()),
                        None => (raw, String::new()),
                    };
                    children.push(XastChild::Instruction { name, value });
                } else if p.starts_with("<!") {
                    // Other declarations: skip to '>'.
                    p.take_until(">");
                    p.expect(">")?;
                } else {
                    children.push(XastChild::Element(parse_element(p)?));
                }
            }
            Some(_) => {
                let text = p.take_until("<");
                // JS parser.js `ontext`: text inside a text element is
                // preserved verbatim; elsewhere it is trimmed and empty
                // text is dropped (root text is ignored).
                match closing {
                    Some(parent) if is_text_elem(parent) => {
                        if !text.is_empty() {
                            children.push(XastChild::Text(text));
                        }
                    }
                    Some(_) => {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            children.push(XastChild::Text(trimmed.to_string()));
                        }
                    }
                    None => {}
                }
            }
        }
    }
}

/// Scan a doctype body, honouring quotes and an internal `[ ... ]` subset.
fn parse_doctype(p: &mut Parser) -> Result<String, String> {
    p.pos += "<!DOCTYPE".chars().count().min(9);
    // Consume the literal case-insensitively already matched; resync:
    // (pos was advanced by 9 for "<!DOCTYPE"/"<!doctype", both length 9.)
    let mut out = String::new();
    let mut bracket_depth = 0i32;
    let mut quote: Option<char> = None;
    while let Some(c) = p.bump() {
        match quote {
            Some(q) => {
                out.push(c);
                if c == q {
                    quote = None;
                }
            }
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                out.push(c);
            }
            None if c == '[' => {
                bracket_depth += 1;
                out.push(c);
            }
            None if c == ']' => {
                bracket_depth -= 1;
                out.push(c);
            }
            None if c == '>' && bracket_depth <= 0 => return Ok(out),
            None => out.push(c),
        }
    }
    Err("parse error: unterminated doctype".into())
}

fn parse_element(p: &mut Parser) -> Result<XastElement, String> {
    p.expect("<")?;
    let mut name = String::new();
    while let Some(c) = p.peek() {
        if c.is_whitespace() || c == '/' || c == '>' {
            break;
        }
        name.push(p.bump().unwrap());
    }
    if name.is_empty() {
        return Err(format!("parse error at {}: empty element name", p.pos));
    }
    let mut attributes: Vec<(String, String)> = Vec::new();
    loop {
        while matches!(p.peek(), Some(c) if c.is_whitespace()) {
            p.bump();
        }
        match p.peek() {
            Some('/') => {
                p.bump();
                p.expect(">")?;
                return Ok(XastElement { name, attributes, children: Vec::new() });
            }
            Some('>') => {
                p.bump();
                let children = parse_children(p, Some(&name))?;
                return Ok(XastElement { name, attributes, children });
            }
            Some(_) => {
                let mut attr_name = String::new();
                while let Some(c) = p.peek() {
                    if c == '=' || c.is_whitespace() || c == '/' || c == '>' {
                        break;
                    }
                    attr_name.push(p.bump().unwrap());
                }
                while matches!(p.peek(), Some(c) if c.is_whitespace()) {
                    p.bump();
                }
                let attr_value = if p.peek() == Some('=') {
                    p.bump();
                    while matches!(p.peek(), Some(c) if c.is_whitespace()) {
                        p.bump();
                    }
                    match p.bump() {
                        Some(q @ ('"' | '\'')) => {
                            let v = p.take_until(&q.to_string());
                            p.expect(&q.to_string())?;
                            v
                        }
                        Some(_) => p.take_until(">"),
                        None => String::new(),
                    }
                } else {
                    String::new()
                };
                attributes.push((attr_name, attr_value));
            }
            None => return Err("parse error: unterminated element".into()),
        }
    }
}

/// Serialize with the equivalent of SVGO `js2svg: { pretty: true }`:
/// 4-space indent, empty elements self-close, no trailing newline.
/// Mirrors JS `lib/stringifier.js`: inside a text element (or a
/// descendant of one) no indent or newline is added and text is
/// emitted verbatim; elsewhere nodes are indented and newline-ended.
pub fn stringify_pretty(root: &XastRoot) -> String {
    let mut out = String::new();
    write_children(&root.children, 0, false, &mut out);
    while out.ends_with('\n') {
        out.pop();
    }
    out
}

fn write_children(children: &[XastChild], depth: usize, in_text: bool, out: &mut String) {
    for child in children {
        write_node(child, depth, in_text, out);
    }
}

fn write_node(node: &XastChild, depth: usize, in_text: bool, out: &mut String) {
    let indent = if in_text { String::new() } else { "    ".repeat(depth) };
    match node {
        XastChild::Text(t) => {
            out.push_str(&indent);
            out.push_str(t);
            if !in_text {
                out.push('\n');
            }
        }
        XastChild::Comment(v) => {
            out.push_str(&indent);
            out.push_str(&format!("<!--{v}-->"));
            if !in_text {
                out.push('\n');
            }
        }
        XastChild::Cdata(v) => {
            out.push_str(&indent);
            out.push_str(&format!("<![CDATA[{v}]]>"));
            out.push('\n');
        }
        XastChild::Doctype(v) => {
            out.push_str(&indent);
            out.push_str(&format!("<!DOCTYPE{v}>"));
            if !in_text {
                out.push('\n');
            }
        }
        XastChild::Instruction { name, value } => {
            out.push_str(&indent);
            if value.is_empty() {
                out.push_str(&format!("<?{name}?>"));
            } else {
                out.push_str(&format!("<?{name} {value}?>"));
            }
            if !in_text {
                out.push('\n');
            }
        }
        XastChild::Element(el) => write_element(el, depth, in_text, out),
    }
}

fn write_element(el: &XastElement, depth: usize, in_text: bool, out: &mut String) {
    let attrs = el
        .attributes
        .iter()
        .map(|(k, v)| format!(" {k}=\"{v}\""))
        .collect::<String>();
    let indent = if in_text { String::new() } else { "    ".repeat(depth) };
    if el.children.is_empty() {
        out.push_str(&format!("{indent}<{}{attrs}/>", el.name));
        // JS appends the pretty newline to tagShortEnd unconditionally.
        out.push('\n');
        return;
    }
    let child_in_text = in_text || is_text_elem(&el.name);
    out.push_str(&format!("{indent}<{}{attrs}>", el.name));
    if !child_in_text {
        out.push('\n');
    }
    write_children(&el.children, depth + 1, child_in_text, out);
    if !child_in_text {
        out.push_str(&"    ".repeat(depth));
    }
    out.push_str(&format!("</{}>", el.name));
    if !in_text {
        out.push('\n');
    }
}

