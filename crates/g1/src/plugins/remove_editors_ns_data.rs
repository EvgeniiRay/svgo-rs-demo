//! Port of `plugins/removeEditorsNSData.js`.
//! Enter-order traversal: on `<svg>`, collect prefixes whose
//! `xmlns:<prefix>` value is an editor namespace and delete that
//! declaration; then strip `prefix:*` attributes and detach
//! `prefix:*` elements for collected prefixes.

use super::collections::EDITOR_NAMESPACES;
use crate::json::JsonValue;
use crate::xast::{XastChild, XastRoot};

pub fn apply(root: &mut XastRoot, params: &JsonValue) -> Result<(), String> {
    let mut namespaces: Vec<String> =
        EDITOR_NAMESPACES.iter().map(|s| s.to_string()).collect();
    if let Some(JsonValue::Array(extra)) = params.get("additionalNamespaces") {
        for item in extra {
            if let Some(s) = item.as_str() {
                namespaces.push(s.to_string());
            }
        }
    }
    let mut prefixes: Vec<String> = Vec::new();
    visit_children(&mut root.children, &namespaces, &mut prefixes);
    Ok(())
}

fn visit_children(
    children: &mut Vec<XastChild>,
    namespaces: &[String],
    prefixes: &mut Vec<String>,
) {
    let mut i = 0;
    while i < children.len() {
        let detach = match &mut children[i] {
            XastChild::Element(el) => {
                // Collect namespace prefixes from the svg element.
                if el.name == "svg" {
                    let mut kept = Vec::new();
                    for (name, value) in std::mem::take(&mut el.attributes) {
                        if let Some(prefix) = name.strip_prefix("xmlns:") {
                            if namespaces.iter().any(|ns| ns == &value) {
                                prefixes.push(prefix.to_string());
                                continue; // delete the xmlns declaration
                            }
                        }
                        kept.push((name, value));
                    }
                    el.attributes = kept;
                }
                // Remove editor attributes, e.g. <* sodipodi:*="">.
                el.attributes.retain(|(name, _)| match name.split_once(':') {
                    Some((prefix, _)) => !prefixes.iter().any(|p| p == prefix),
                    None => true,
                });
                // Remove editor elements, e.g. <sodipodi:*>.
                let is_editor_element = match el.name.split_once(':') {
                    Some((prefix, _)) => prefixes.iter().any(|p| p == prefix),
                    None => false,
                };
                if is_editor_element {
                    true
                } else {
                    visit_children(&mut el.children, namespaces, prefixes);
                    false
                }
            }
            _ => false,
        };
        if detach {
            children.remove(i);
        } else {
            i += 1;
        }
    }
}
