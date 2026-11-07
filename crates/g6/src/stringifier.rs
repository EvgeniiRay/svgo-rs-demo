//! Stringifier mirroring `reference/svgo/lib/stringifier.js` with
//! defaults `pretty: false, useShortTags: true, indent: 4, finalNewline: false`.

use crate::xast::{is_text_elem, XastChild, XastElement, XastRoot};

#[derive(Debug, Clone)]
pub struct StringifyOptions {
    pub pretty: bool,
    pub indent: String,
    pub use_short_tags: bool,
    pub final_newline: bool,
}

impl Default for StringifyOptions {
    fn default() -> Self {
        Self { pretty: false, indent: "    ".to_string(), use_short_tags: true, final_newline: false }
    }
}

pub fn stringify_svg(root: &XastRoot, options: &StringifyOptions) -> String {
    let mut out = String::new();
    stringify_children(&root.children, 0, false, options, &mut out);
    if options.final_newline && !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn encode_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('\'', "&apos;").replace('"', "&quot;").replace('>', "&gt;").replace('<', "&lt;")
}
fn encode_attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;").replace('>', "&gt;").replace('<', "&lt;")
}

fn indent_of(options: &StringifyOptions, level: usize, in_text: bool) -> String {
    if options.pretty && !in_text { options.indent.repeat(level) } else { String::new() }
}
fn eol(options: &StringifyOptions, in_text: bool) -> &'static str {
    if options.pretty && !in_text { "\n" } else { "" }
}

fn stringify_children(children: &[XastChild], level: usize, in_text: bool, options: &StringifyOptions, out: &mut String) {
    for child in children {
        match child {
            XastChild::Element(el) => stringify_element(el, level, in_text, options, out),
            XastChild::Text { value } => {
                out.push_str(&indent_of(options, level, in_text));
                out.push_str(&encode_text(value));
                if !in_text { out.push_str(eol(options, false)); }
            }
            XastChild::Doctype { doctype, .. } => {
                out.push_str("<!DOCTYPE");
                out.push_str(doctype);
                out.push('>');
                out.push_str(eol(options, false));
            }
            XastChild::Instruction { name, value } => {
                out.push_str("<?");
                out.push_str(name);
                out.push(' ');
                out.push_str(value);
                out.push_str("?>");
                out.push_str(eol(options, false));
            }
            XastChild::Comment { value } => {
                out.push_str("<!--");
                out.push_str(value);
                out.push_str("-->");
                out.push_str(eol(options, false));
            }
            XastChild::Cdata { value } => {
                out.push_str(&indent_of(options, level, in_text));
                out.push_str("<![CDATA[");
                out.push_str(value);
                out.push_str("]]>");
                out.push_str(eol(options, in_text));
            }
        }
    }
}

fn stringify_element(el: &XastElement, level: usize, in_text: bool, options: &StringifyOptions, out: &mut String) {
    let mut attrs = String::new();
    for (name, value) in &el.attributes {
        attrs.push(' ');
        attrs.push_str(name);
        attrs.push_str("=\"");
        attrs.push_str(&encode_attr(value));
        attrs.push('"');
    }
    let open_indent = indent_of(options, level, in_text);
    if el.children.is_empty() {
        out.push_str(&open_indent);
        out.push('<');
        out.push_str(&el.name);
        out.push_str(&attrs);
        if options.use_short_tags {
            out.push_str("/>");
            // JS appends the pretty newline to tagShortEnd unconditionally.
            out.push_str(eol(options, false));
        } else {
            out.push('>');
            out.push_str(eol(options, in_text));
            out.push_str(&open_indent);
            out.push_str("</");
            out.push_str(&el.name);
            out.push('>');
            out.push_str(eol(options, in_text));
        }
        return;
    }
    let is_text = is_text_elem(&el.name);
    let child_in_text = in_text || is_text;
    // JS: a text element suppresses the newline after its open tag and
    // the indent before its close tag, but its close tag still ends with
    // a newline; descendants of a text element use plain tag ends.
    out.push_str(&open_indent);
    out.push('<');
    out.push_str(&el.name);
    out.push_str(&attrs);
    out.push('>');
    if !child_in_text { out.push_str(eol(options, false)); }
    stringify_children(&el.children, level + 1, child_in_text, options, out);
    if !child_in_text { out.push_str(&indent_of(options, level, false)); }
    out.push_str("</");
    out.push_str(&el.name);
    out.push('>');
    if !in_text { out.push_str(eol(options, false)); }
}
