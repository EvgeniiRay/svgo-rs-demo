//! Port of `plugins/removeXMLProcInst.js`: drop `<?xml ...?>` only;
//! other instructions (e.g. `xml-stylesheet`) are kept.

use crate::xast::{XastChild, XastRoot};

pub fn apply(root: &mut XastRoot) -> Result<(), String> {
    strip(&mut root.children);
    Ok(())
}

fn strip(children: &mut Vec<XastChild>) {
    children.retain(
        |c| !matches!(c, XastChild::Instruction { name, .. } if name == "xml"),
    );
    for child in children.iter_mut() {
        if let XastChild::Element(el) = child {
            strip(&mut el.children);
        }
    }
}
