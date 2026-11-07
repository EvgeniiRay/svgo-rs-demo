//! Port of `reference/svgo/plugins/removeTitle.js`.

use crate::xast::{XastChild, XastElement, XastRoot};

pub const NAME: &str = "removeTitle";
pub const DESCRIPTION: &str = "removes <title>";

pub fn apply(root: &mut XastRoot) {
    filter_children(&mut root.children);
}

fn filter_children(children: &mut Vec<XastChild>) {
    children.retain(|child| !matches!(child, XastChild::Element(el) if el.name == "title"));
    for child in children.iter_mut() {
        if let XastChild::Element(el) = child {
            filter_element(el);
        }
    }
}

fn filter_element(el: &mut XastElement) {
    filter_children(&mut el.children);
}
