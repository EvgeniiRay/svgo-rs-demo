//! Port of plugins/removeNonInheritableGroupAttrs.js

use crate::collections::{INHERITABLE_ATTRS, PRESENTATION_ATTRS, PRESENTATION_NONINHERITABLE_GROUP_ATTRS};
use crate::xast::{walk_elements, Root};

pub fn apply(root: &mut Root) {
    walk_elements(&mut root.children, &mut |node| {
        if node.name == "g" {
            for name in node.attr_names() {
                if PRESENTATION_ATTRS.contains(&name.as_str())
                    && !INHERITABLE_ATTRS.contains(&name.as_str())
                    && !PRESENTATION_NONINHERITABLE_GROUP_ATTRS.contains(&name.as_str())
                {
                    node.remove(&name);
                }
            }
        }
    });
}
