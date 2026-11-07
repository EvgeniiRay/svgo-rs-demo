//! Port of `plugins/removeMetadata.js`: drop every `<metadata>` element.

use crate::xast::{XastChild, XastRoot};

pub fn apply(root: &mut XastRoot) -> Result<(), String> {
    strip(&mut root.children);
    Ok(())
}

fn strip(children: &mut Vec<XastChild>) {
    children.retain(|c| !matches!(c, XastChild::Element(el) if el.name == "metadata"));
    for child in children.iter_mut() {
        if let XastChild::Element(el) = child {
            strip(&mut el.children);
        }
    }
}
