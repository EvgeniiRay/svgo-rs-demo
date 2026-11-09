//! Port of `reference/svgo/plugins/removeUnusedNS.js`.

use crate::xast::{XastChild, XastRoot};
use std::collections::HashSet;

pub const NAME: &str = "removeUnusedNS";
pub const DESCRIPTION: &str = "removes unused namespaces declaration";

pub fn apply(root: &mut XastRoot) {
    let mut unused: HashSet<String> = HashSet::new();
    // Root children: parent is root, so a top-level <svg> collects/removes.
    for child in root.children.iter_mut() {
        walk(child, true, &mut unused);
    }
}

fn walk(node: &mut XastChild, parent_is_root: bool, unused: &mut HashSet<String>) {
    let XastChild::Element(el) = node else { return };
    let is_root_svg = el.name == "svg" && parent_is_root;
    // enter
    if is_root_svg {
        for (name, _) in &el.attributes {
            if let Some(local) = name.strip_prefix("xmlns:") {
                unused.insert(local.to_string());
            }
        }
    }
    if !unused.is_empty() {
        if let Some((ns, _)) = el.name.split_once(':') {
            unused.remove(ns);
        }
        // Collect first to avoid borrowing el while mutating the set only
        // (no element mutation here, so a direct scan is fine).
        let used: Vec<String> = el
            .attributes
            .iter()
            .filter_map(|(name, _)| name.split_once(':').map(|(ns, _)| ns.to_string()))
            .collect();
        for ns in &used {
            unused.remove(ns);
        }
    }
    for child in el.children.iter_mut() {
        walk(child, false, unused);
    }
    // exit
    if is_root_svg {
        let unused_snapshot: Vec<String> = unused.iter().cloned().collect();
        for ns in unused_snapshot {
            el.remove_attr(&format!("xmlns:{ns}"));
        }
    }
}
