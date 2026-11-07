pub mod cleanup_ids;
pub mod inline_styles;
pub mod merge_styles;
pub mod minify_styles;
pub mod remove_useless_defs;

use crate::dom::Node;
use crate::json::Json;

/// Dispatch to one of this group's plugins. Returns false for unknown names.
pub fn apply(root: &mut Vec<Node>, name: &str, params: &Json) -> bool {
    match name {
        "mergeStyles" => merge_styles::apply(root),
        "inlineStyles" => inline_styles::apply(root, params),
        "minifyStyles" => minify_styles::apply(root, params),
        "cleanupIds" => cleanup_ids::apply(root, params),
        "removeUselessDefs" => remove_useless_defs::apply(root),
        _ => return false,
    }
    true
}

/// Collect element uids in document order, skipping `foreignObject` subtrees.
pub fn collect_uids_skip_foreign(nodes: &[Node], out: &mut Vec<u64>) {
    for n in nodes {
        if let Node::Element(e) = n {
            out.push(e.uid);
            if e.name != "foreignObject" {
                collect_uids_skip_foreign(&e.children, out);
            }
        }
    }
}
