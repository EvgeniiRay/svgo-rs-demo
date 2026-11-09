use std::sync::atomic::{AtomicU64, Ordering};
static UID: AtomicU64 = AtomicU64::new(1);
pub fn next_uid() -> u64 { UID.fetch_add(1, Ordering::Relaxed) }

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Element(Element),
    Text(String),
    CData(String),
    Comment(String),
    Doctype(String),
    Instruction { name: String, value: String },
}
impl Node {
    pub fn as_element(&self) -> Option<&Element> { if let Node::Element(e)=self {Some(e)} else {None} }
    pub fn as_element_mut(&mut self) -> Option<&mut Element> { if let Node::Element(e)=self {Some(e)} else {None} }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub uid: u64,
    pub name: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Node>,
}
impl Element {
    pub fn new(name: &str) -> Self { Self { uid: next_uid(), name: name.to_string(), attributes: Vec::new(), children: Vec::new() } }
    pub fn get(&self, n: &str) -> Option<&str> { self.attributes.iter().find(|(k,_)| k==n).map(|(_,v)| v.as_str()) }
    pub fn has(&self, n: &str) -> bool { self.get(n).is_some() }
    pub fn set(&mut self, n: &str, v: &str) {
        if let Some(slot) = self.attributes.iter_mut().find(|(k,_)| k==n) { slot.1 = v.to_string(); }
        else { self.attributes.push((n.to_string(), v.to_string())); }
    }
    pub fn remove_attr(&mut self, n: &str) { self.attributes.retain(|(k,_)| k!=n); }
    pub fn text_content(&self) -> String {
        let mut s = String::new();
        for c in &self.children { match c { Node::Text(t)|Node::CData(t) => s.push_str(t), Node::Element(e) => s.push_str(&e.text_content()), _ => {} } }
        s
    }
}
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Root { pub children: Vec<Node> }

pub fn is_text_elem(name: &str) -> bool {
    matches!(name, "a"|"altGlyph"|"altGlyphDef"|"altGlyphItem"|"glyph"|"glyphRef"|"text"|"textPath"|"tref"|"tspan"|"pre"|"title")
}
pub fn detach_by_uid(children: &mut Vec<Node>, uid: u64) -> bool {
    if let Some(pos) = children.iter().position(|c| matches!(c, Node::Element(e) if e.uid==uid)) { children.remove(pos); return true; }
    for c in children.iter_mut() { if let Node::Element(e)=c { if detach_by_uid(&mut e.children, uid) { return true; } } }
    false
}
pub fn find_by_uid<'a>(children: &'a [Node], uid: u64) -> Option<&'a Element> {
    for c in children { if let Node::Element(e)=c { if e.uid==uid { return Some(e); } if let Some(f)=find_by_uid(&e.children, uid) { return Some(f); } } }
    None
}
pub fn collect_elements(children: &[Node], out: &mut Vec<(u64,String)>) {
    for c in children { if let Node::Element(e)=c { out.push((e.uid, e.name.clone())); collect_elements(&e.children, out); } }
}
