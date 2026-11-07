use crate::plugins::Plugin;
use crate::xast::{Node, Root};

#[derive(Debug, Clone, Copy)]
pub struct RemoveEmptyTextParams { pub text: bool, pub tspan: bool, pub tref: bool }
impl Default for RemoveEmptyTextParams { fn default() -> Self { Self { text: true, tspan: true, tref: true } } }

pub struct RemoveEmptyText { pub params: RemoveEmptyTextParams }
impl Default for RemoveEmptyText { fn default() -> Self { Self { params: RemoveEmptyTextParams::default() } } }
impl Plugin for RemoveEmptyText {
    fn name(&self) -> &'static str { "removeEmptyText" }
    fn apply(&self, root: &mut Root) { let p = self.params; walk(&mut root.children, &p); }
}
fn walk(children: &mut Vec<Node>, p: &RemoveEmptyTextParams) {
    let mut i = 0;
    while i < children.len() {
        let remove = if let Node::Element(e) = &children[i] {
            (p.text && e.name=="text" && e.children.is_empty())
            || (p.tspan && e.name=="tspan" && e.children.is_empty())
            || (p.tref && e.name=="tref" && !e.has("xlink:href"))
        } else { false };
        if remove { children.remove(i); continue; }
        if let Node::Element(e) = &mut children[i] { walk(&mut e.children, p); }
        i += 1;
    }
}
