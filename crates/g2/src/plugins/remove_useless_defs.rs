//! Port of `plugins/removeUselessDefs.js`.

use crate::collections::is_non_rendering;
use crate::dom::{Element, Node};

pub fn apply(root: &mut Vec<Node>) {
    process_children(root);
}

fn process_children(children: &mut Vec<Node>) {
    let mut i = 0;
    while i < children.len() {
        let remove = match &mut children[i] {
            Node::Element(e) => !visit_element(e),
            _ => false,
        };
        if remove {
            children.remove(i);
        } else {
            i += 1;
        }
    }
}

/// Returns false if the element should be detached from its parent.
fn visit_element(e: &mut Element) -> bool {
    if e.name == "defs" || (is_non_rendering(&e.name) && e.get("id").is_none()) {
        let mut useful: Vec<Node> = Vec::new();
        collect_useful(e, &mut useful);
        if useful.is_empty() {
            return false;
        }
        e.children = useful;
    }
    process_children(&mut e.children);
    true
}

fn collect_useful(node: &Element, out: &mut Vec<Node>) {
    for child in &node.children {
        if let Node::Element(child_el) = child {
            if child_el.get("id").is_some() || child_el.name == "style" {
                out.push(child.clone());
            } else {
                collect_useful(child_el, out);
            }
        }
    }
}
