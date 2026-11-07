use crate::xast::{is_text_elem, Element, Node, Root};

fn enc(s: &str, attr: bool) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' if attr => o.push_str("&quot;"),
            '\'' if !attr => o.push_str("&apos;"),
            '"' if !attr => o.push_str("&quot;"),
            _ => o.push(c),
        }
    }
    o
}
#[derive(Clone)]
struct State { level: usize, text_context: bool }
impl State {
    fn indent(&self) -> String { if self.text_context { String::new() } else { "    ".repeat(self.level.saturating_sub(1)) } }
}
pub fn stringify_svg(root: &Root) -> String {
    let mut st = State { level: 0, text_context: false };
    stringify_children(&root.children, &mut st)
}
fn stringify_children(children: &[Node], st: &mut State) -> String {
    st.level += 1;
    let mut s = String::new();
    for item in children {
        match item {
            Node::Element(e) => s.push_str(&stringify_element(e, st)),
            Node::Text(t) => { let ind = st.indent(); s.push_str(&ind); s.push_str(&enc(t, false)); if !st.text_context { s.push('\n'); } }
            Node::CData(t) => { let ind = st.indent(); s.push_str(&ind); s.push_str("<![CDATA["); s.push_str(t); s.push_str("]]>\n"); }
            Node::Comment(t) => { s.push_str("<!--"); s.push_str(t); s.push_str("-->\n"); }
            Node::Doctype(d) => { s.push_str("<!DOCTYPE"); s.push_str(d); s.push_str(">\n"); }
            Node::Instruction { name, value } => { s.push_str("<?"); s.push_str(name); s.push(' '); s.push_str(value); s.push_str("?>\n"); }
        }
    }
    st.level -= 1; s
}
fn attrs(e: &Element) -> String {
    let mut s = String::new();
    for (k, v) in &e.attributes { s.push(' '); s.push_str(k); s.push_str("=\""); s.push_str(&enc(v, true)); s.push('"'); }
    s
}
fn stringify_element(e: &Element, st: &mut State) -> String {
    if e.children.is_empty() {
        return format!("{}<{}{}/>\n", st.indent(), e.name, attrs(e));
    }
    let in_text = st.text_context;
    let is_text = is_text_elem(&e.name);
    let open_indent = if in_text { String::new() } else { st.indent() };
    let close_indent = if in_text || is_text { String::new() } else { st.indent() };
    let prev = st.text_context;
    if is_text && !in_text { st.text_context = true; }
    let children = stringify_children(&e.children, st);
    if is_text && !in_text { st.text_context = prev; }
    if in_text {
        format!("{}<{}{}>{}</{}>", open_indent, e.name, attrs(e), children, e.name)
    } else if is_text {
        format!("{}<{}{}>{}</{}>\n", open_indent, e.name, attrs(e), children, e.name)
    } else {
        format!("{}<{}{}>\n{}{}</{}>\n", open_indent, e.name, attrs(e), children, close_indent, e.name)
    }
}
