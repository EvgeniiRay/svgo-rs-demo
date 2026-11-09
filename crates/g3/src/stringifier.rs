//! Stringifier mirroring lib/stringifier.js with js2svg { pretty: true,
//! indent: 4, useShortTags: true, eol: lf, finalNewline: false }.

use crate::collections::TEXT_ELEMS;
use crate::xast::{Child, Element, Root};

fn encode_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('\'', "&apos;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;")
}
fn encode_attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;")
}
fn attrs(el: &Element) -> String {
    let mut out = String::new();
    for (n, v) in &el.attributes {
        out.push(' ');
        out.push_str(n);
        out.push_str("=\"");
        out.push_str(&encode_attr(v));
        out.push('"');
    }
    out
}

pub fn stringify_svg(root: &Root) -> String {
    let mut out = String::new();
    stringify_children(&root.children, 0, false, &mut out);
    out
}

fn indent(depth: usize, text_context: bool) -> String {
    if text_context { String::new() } else { "    ".repeat(depth) }
}

fn stringify_children(children: &[Child], depth: usize, text_context: bool, out: &mut String) {
    for child in children {
        match child {
            Child::Element(el) => stringify_element(el, depth, text_context, out),
            Child::Text(v) => {
                if text_context {
                    out.push_str(&encode_text(v));
                } else {
                    out.push_str(&indent(depth, false));
                    out.push_str(&encode_text(v));
                    out.push('\n');
                }
            }
            Child::Cdata(v) => {
                out.push_str(&indent(depth, text_context));
                out.push_str("<![CDATA[");
                out.push_str(v);
                out.push_str("]]>\n");
            }
            Child::Comment(v) => {
                out.push_str("<!--");
                out.push_str(v);
                out.push_str("-->\n");
            }
            Child::Doctype { data, .. } => {
                out.push_str("<!DOCTYPE");
                out.push_str(data);
                out.push_str(">\n");
            }
            Child::Instruction { name, value } => {
                out.push_str("<?");
                out.push_str(name);
                out.push(' ');
                out.push_str(value);
                out.push_str("?>\n");
            }
        }
    }
}

fn stringify_element(el: &Element, depth: usize, text_context: bool, out: &mut String) {
    if el.children.is_empty() {
        out.push_str(&indent(depth, text_context));
        out.push('<');
        out.push_str(&el.name);
        out.push_str(&attrs(el));
        out.push_str("/>");
        if !text_context {
            out.push('\n');
        }
        return;
    }
    let is_text = TEXT_ELEMS.contains(&el.name.as_str());
    let ctx = text_context || is_text;
    out.push_str(&indent(depth, text_context));
    out.push('<');
    out.push_str(&el.name);
    out.push_str(&attrs(el));
    out.push('>');
    if !ctx {
        out.push('\n');
    }
    stringify_children(&el.children, depth + 1, ctx, out);
    if !ctx {
        out.push_str(&indent(depth, false));
    }
    out.push_str("</");
    out.push_str(&el.name);
    out.push('>');
    if !text_context {
        out.push('\n');
    }
}
