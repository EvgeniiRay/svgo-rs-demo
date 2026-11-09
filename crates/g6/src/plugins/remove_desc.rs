//! Port of `reference/svgo/plugins/removeDesc.js`.

use crate::xast::{XastChild, XastElement, XastRoot};

pub const NAME: &str = "removeDesc";
pub const DESCRIPTION: &str = "removes <desc>";

#[derive(Debug, Clone, Copy)]
pub struct RemoveDescParams {
    pub remove_any: bool,
}

impl Default for RemoveDescParams {
    fn default() -> Self {
        Self { remove_any: false }
    }
}

/// `^(Created with|Created using)`
fn is_standard_desc(el: &XastElement) -> bool {
    match el.children.first() {
        Some(XastChild::Text { value }) => {
            value.starts_with("Created with") || value.starts_with("Created using")
        }
        _ => false,
    }
}

pub fn apply(root: &mut XastRoot, params: &RemoveDescParams) {
    filter_children(&mut root.children, params);
}

fn should_remove(el: &XastElement, params: &RemoveDescParams) -> bool {
    params.remove_any || el.children.is_empty() || is_standard_desc(el)
}

fn filter_children(children: &mut Vec<XastChild>, params: &RemoveDescParams) {
    children.retain(|child| {
        !matches!(child, XastChild::Element(el) if el.name == "desc" && should_remove(el, params))
    });
    for child in children.iter_mut() {
        if let XastChild::Element(el) = child {
            filter_children(&mut el.children, params);
        }
    }
}
