use crate::collections::{inheritable, path_elem};
use crate::plugins::Plugin;
use crate::xast::{Node, Root};

pub struct MoveElemsAttrsToGroup;
impl Plugin for MoveElemsAttrsToGroup {
    fn name(&self) -> &'static str { "moveElemsAttrsToGroup" }
    fn apply(&self, root: &mut Root) {
        let deopt = has_style(&root.children);
        walk(&mut root.children, deopt);
    }
}
fn has_style(children: &[Node]) -> bool {
    children.iter().any(|c| if let Node::Element(e)=c { e.name=="style" || has_style(&e.children) } else { false })
}
fn walk(children: &mut [Node], deopt: bool) {
    for c in children.iter_mut() {
        if let Node::Element(e)=c { walk(&mut e.children, deopt); process(e, deopt); }
    }
}
fn process(node: &mut crate::xast::Element, deopt: bool) {
    if node.name!="g" || node.children.len()<=1 || deopt { return; }
    let mut common: Vec<(String,String)> = Vec::new();
    let mut initial = true;
    let mut every_child_is_path = true;
    for child in &node.children {
        if let Node::Element(ch)=child {
            if !path_elem(&ch.name) { every_child_is_path=false; }
            if initial {
                initial=false;
                for (n,v) in &ch.attributes { if inheritable(n) { common.push((n.clone(), v.clone())); } }
            } else {
                common.retain(|(n,v)| ch.get(n)==Some(v.as_str()));
            }
        }
    }
    if node.has("filter") || node.has("clip-path") || node.has("mask") { common.retain(|(n,_)| n!="transform"); }
    if every_child_is_path { common.retain(|(n,_)| n!="transform"); }
    for (n,v) in &common {
        if n=="transform" {
            if let Some(existing)=node.get("transform").map(|s| s.to_string()) { node.set("transform", &format!("{} {}", existing, v)); }
            else { node.set("transform", v); }
        } else { node.set(n, v); }
    }
    let names: Vec<String> = common.iter().map(|(n,_)| n.clone()).collect();
    for child in &mut node.children {
        if let Node::Element(ch)=child { for n in &names { ch.remove_attr(n); } }
    }
}
