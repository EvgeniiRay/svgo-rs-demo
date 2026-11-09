//! Minimal Rust port of a subset of SVGO.
//!
//! Mirrors the JS architecture at a small scale: an XAST DOM
//! (`xast`), a sax-like parser and a pretty stringifier, and visitor-style
//! plugins under `plugins` (analogous to `lib/xast.js` + `plugins/*.js`).

pub mod json;
pub mod plugins;
pub mod xast;

use json::JsonValue;

/// Parse `input`, run a single named plugin, and serialize with the
/// equivalent of SVGO `js2svg: { pretty: true }`.
pub fn optimize_with_plugin(
    input: &str,
    plugin_name: &str,
    params_json: &str,
) -> Result<String, String> {
    let mut root = xast::parse(input)?;
    let params = if params_json.trim().is_empty() {
        JsonValue::Object(Vec::new())
    } else {
        JsonValue::parse(params_json)?
    };
    plugins::apply_plugin(plugin_name, &mut root, &params)?;
    Ok(xast::stringify_pretty(&root))
}
