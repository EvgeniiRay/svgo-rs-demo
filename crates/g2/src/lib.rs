pub mod collections;
pub mod css;
pub mod dom;
pub mod json;
pub mod plugins;
pub mod selector;
pub mod tools;

use dom::Node;

/// Parse an SVG string into a root node list.
pub fn parse(svg: &str) -> Vec<Node> {
    dom::parse(svg)
}

/// Serialize with SVGO `js2svg: { pretty: true }` semantics.
pub fn stringify_pretty(nodes: &[Node]) -> String {
    dom::stringify_pretty(nodes)
}

/// Parse, apply a single named plugin, and serialize (pretty).
/// Returns `None` for an unknown plugin name.
pub fn optimize_plugin(svg: &str, name: &str, params_json: &str) -> Option<String> {
    let params = json::parse(params_json).unwrap_or(json::Json::Null);
    let mut nodes = parse(svg);
    if !plugins::apply(&mut nodes, name, &params) {
        return None;
    }
    Some(stringify_pretty(&nodes))
}
