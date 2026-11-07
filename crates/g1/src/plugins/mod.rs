//! Visitor-style plugins, ported from `reference/svgo/plugins/*.js`.
//! Each `apply` corresponds to the JS plugin's `fn` visitor object.

pub mod cleanup_attrs;
pub mod collections;
pub mod remove_comments;
pub mod remove_doctype;
pub mod remove_editors_ns_data;
pub mod remove_metadata;
pub mod remove_xml_proc_inst;

use crate::json::JsonValue;
use crate::xast::XastRoot;

pub const NAMES: [&str; 6] = [
    "removeDoctype",
    "removeXMLProcInst",
    "removeComments",
    "removeMetadata",
    "removeEditorsNSData",
    "cleanupAttrs",
];

pub fn apply_plugin(name: &str, root: &mut XastRoot, params: &JsonValue) -> Result<(), String> {
    match name {
        "removeDoctype" => remove_doctype::apply(root),
        "removeXMLProcInst" => remove_xml_proc_inst::apply(root),
        "removeComments" => remove_comments::apply(root, params),
        "removeMetadata" => remove_metadata::apply(root),
        "removeEditorsNSData" => remove_editors_ns_data::apply(root, params),
        "cleanupAttrs" => cleanup_attrs::apply(root, params),
        other => Err(format!("unknown plugin: {other}")),
    }
}
