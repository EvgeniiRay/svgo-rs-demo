use crate::collections::{path_elem, references_prop};
use crate::plugins::Plugin;
use crate::tools::includes_url_reference;
use crate::xast::{Node, Root};

pub struct MoveGroupAttrsToElems;
impl Plugin for MoveGroupAttrsToElems {
    fn name(&self) -> &'static str { "moveGroupAttrsToElems" }
    fn apply(&self, root: &mut Root) { walk(&mut root.children); }
}
fn is_path_group_text(name: &str) -> bool { path_elem(name) || name=="g" || name=="text" }
fn walk(children: &mut [Node]) {
    for c in children.iter_mut() {
        if let Node::Element(e)=c {
            process(e);
            walk(&mut e.children);
        }
    }
}
fn process(node: &mut crate::xast::Element) {
    if node.name!="g" || node.children.is_empty() || !node.has("transform") { return; }
    let has_ref = node.attributes.iter().any(|(n,v)| references_prop(n) && includes_url_reference(v));
    if has_ref { return; }
    let all_ok = node.children.iter().all(|ch| matches!(ch, Node::Element(el) if is_path_group_text(&el.name) && !el.has("id")));
    if !all_ok { return; }
    let value = node.get("transform").unwrap().to_string();
    for child in &mut node.children {
        if let Node::Element(ch)=child {
            if let Some(existing)=ch.get("transform").map(|s| s.to_string()) { ch.set("transform", &format!("{} {}", value, existing)); }
            else { ch.set("transform", &value); }
        }
    }
    node.remove_attr("transform");
}
