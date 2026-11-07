pub mod dom;
pub mod json;
pub mod path;
pub mod plugins;
pub mod style;
pub mod tools;
pub mod transforms;

use dom::Document;
use json::Json;

/// Run a single plugin (by SVGO name) on an SVG string and return the
/// pretty-printed output, mirroring `optimize` with `js2svg: { pretty: true }`.
pub fn optimize_with_plugin(input: &str, plugin_name: &str, params: &Json) -> String {
    let mut doc = dom::parse(input);
    plugins::run(&mut doc, plugin_name, params);
    dom::stringify(&doc)
}

pub fn parse_and_run(doc: &mut Document, plugin_name: &str, params: &Json) {
    plugins::run(doc, plugin_name, params);
}
