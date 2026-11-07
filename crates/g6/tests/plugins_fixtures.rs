//! Fixture tests ported from `reference/svgo/test/plugins/_index.test.js`:
//! each `<plugin>.<index>.svg.txt` holds `original @@@ should @@@ params?`
//! (optionally preceded by a description and `===`). The plugin is run
//! twice (idempotence / multipass) and compared after trimming.

use std::path::PathBuf;
use svgo_g6::plugins::cleanup_list_of_values::CleanupListOfValuesParams;
use svgo_g6::plugins::remove_desc::RemoveDescParams;
use svgo_g6::{optimize_with_plugin, CleanupListOfValues, Plugin, RemoveDesc, RemoveDimensions, RemoveTitle, RemoveUnusedNs};

fn normalize(s: &str) -> String {
    s.trim().replace("\r\n", "\n")
}

/// Split on a whitespace-surrounded delimiter, like JS `split(/\s*@@@\s*/)`.
fn split_delim(text: &str, delim: &str) -> Vec<String> {
    text.split(delim).map(|p| p.trim().to_string()).collect()
}

fn bool_param(params: &str, key: &str, default: bool) -> bool {
    // Minimal extraction for the flat JSON param objects used in fixtures.
    let needle = format!("\"{key}\"");
    if let Some(idx) = params.find(&needle) {
        let rest = &params[idx + needle.len()..];
        if rest.contains("true") && (!rest.contains("false") || rest.find("true") < rest.find("false")) {
            // Only trust it if `true` appears before any closing brace/comma noise:
            let after_colon = rest.split(':').nth(1).unwrap_or("").trim_start();
            return after_colon.starts_with("true");
        }
        let after_colon = rest.split(':').nth(1).unwrap_or("").trim_start();
        return after_colon.starts_with("true");
    }
    default
}

fn float_param(params: &str, key: &str, default: usize) -> usize {
    let needle = format!("\"{key}\"");
    if let Some(idx) = params.find(&needle) {
        if let Some(after) = params[idx + needle.len()..].split(':').nth(1) {
            let num: String = after.trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(v) = num.parse() {
                return v;
            }
        }
    }
    default
}

fn plugin_for(name: &str, params: &str) -> Box<dyn Plugin> {
    match name {
        "removeUnusedNS" => Box::new(RemoveUnusedNs),
        "removeTitle" => Box::new(RemoveTitle),
        "removeDimensions" => Box::new(RemoveDimensions),
        "removeDesc" => Box::new(RemoveDesc(RemoveDescParams {
            remove_any: bool_param(params, "removeAny", false),
        })),
        "cleanupListOfValues" => Box::new(CleanupListOfValues(CleanupListOfValuesParams {
            float_precision: float_param(params, "floatPrecision", 3),
            leading_zero: bool_param(params, "leadingZero", true),
            default_px: bool_param(params, "defaultPx", true),
            convert_to_px: bool_param(params, "convertToPx", true),
        })),
        other => panic!("unknown plugin {other}"),
    }
}

fn run_fixture(name: &str, index: &str) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../reference/svgo/test/plugins");
    let path = dir.join(format!("{name}.{index}.svg.txt"));
    let data = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    let normalized = normalize(&data);
    // Remove description: items split on /\s*===\s*/
    let items = split_delim(&normalized, "===");
    let test = if items.len() == 2 { items[1].clone() } else { items[0].clone() };
    let parts = split_delim(&test, "@@@");
    assert!(parts.len() >= 2, "{name}.{index}: malformed fixture");
    let original = &parts[0];
    let should = &parts[1];
    let params = parts.get(2).map(|s| s.as_str()).unwrap_or("{}");
    let plugin = plugin_for(name, params);
    // Multipass idempotence check (2 passes), as in the JS harness.
    let mut last = original.clone();
    for pass in 0..2 {
        let result = optimize_with_plugin(&last, plugin.as_ref())
            .unwrap_or_else(|e| panic!("{name}.{index} pass {pass}: {e}"));
        assert_eq!(
            normalize(&result),
            normalize(should),
            "{name}.{index} pass {pass} mismatch"
        );
        last = result;
    }
}

macro_rules! fixture {
    ($fn_name:ident, $name:expr, $index:expr) => {
        #[test]
        fn $fn_name() {
            run_fixture($name, $index);
        }
    };
}

fixture!(remove_unused_ns_01, "removeUnusedNS", "01");
fixture!(remove_unused_ns_02, "removeUnusedNS", "02");
fixture!(remove_unused_ns_03, "removeUnusedNS", "03");
fixture!(remove_unused_ns_04, "removeUnusedNS", "04");
fixture!(remove_unused_ns_05, "removeUnusedNS", "05");
fixture!(remove_unused_ns_06, "removeUnusedNS", "06");
fixture!(remove_unused_ns_07, "removeUnusedNS", "07");
fixture!(cleanup_list_of_values_01, "cleanupListOfValues", "01");
fixture!(cleanup_list_of_values_02, "cleanupListOfValues", "02");
fixture!(cleanup_list_of_values_03, "cleanupListOfValues", "03");
fixture!(remove_title_01, "removeTitle", "01");
fixture!(remove_desc_01, "removeDesc", "01");
fixture!(remove_dimensions_01, "removeDimensions", "01");
fixture!(remove_dimensions_02, "removeDimensions", "02");
fixture!(remove_dimensions_03, "removeDimensions", "03");
fixture!(remove_dimensions_04, "removeDimensions", "04");
fixture!(remove_dimensions_05, "removeDimensions", "05");

#[test]
fn remove_dimensions_edges() {
    // Non-numeric dimensions keep width/height and gain no viewBox.
    let input = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100px\" height=\"50\"/>";
    let out = optimize_with_plugin(input, &RemoveDimensions).unwrap();
    assert!(out.contains("width=\"100px\""), "non-numeric width kept: {out}");
    assert!(!out.contains("viewBox"), "no viewBox for NaN width: {out}");
    // Only one dimension without viewBox: untouched.
    let input = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\"/>";
    let out = optimize_with_plugin(input, &RemoveDimensions).unwrap();
    assert!(out.contains("width=\"100\""), "single width kept: {out}");
    // Nested svg is handled too.
    let input = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"><svg width=\"8\" height=\"4\"/></svg>";
    let out = optimize_with_plugin(input, &RemoveDimensions).unwrap();
    assert!(out.contains("viewBox=\"0 0 8 4\""), "nested svg gains viewBox: {out}");
}

#[test]
fn remove_desc_variants() {
    // Empty desc removed; non-standard kept; standard removed; removeAny removes all.
    let input = "<svg xmlns=\"http://www.w3.org/2000/svg\"><desc/><desc>Custom note</desc><desc>Created using Foo</desc></svg>";
    let out = optimize_with_plugin(input, &RemoveDesc::default()).unwrap();
    assert!(out.contains("Custom note"), "custom desc kept: {out}");
    assert!(!out.contains("Created using"), "standard desc removed: {out}");
    let out_any = optimize_with_plugin(input, &RemoveDesc(RemoveDescParams { remove_any: true })).unwrap();
    assert!(!out_any.contains("Custom note"), "removeAny removes custom: {out_any}");
}

#[test]
fn remove_desc_nested_title_and_cleanup_params() {
    let input = "<svg xmlns=\"http://www.w3.org/2000/svg\"><g><title>t</title></g></svg>";
    let out = optimize_with_plugin(input, &RemoveTitle).unwrap();
    assert!(!out.contains("<title>"), "nested title removed: {out}");
    // cleanupListOfValues with explicit params: no leading-zero removal, keep px.
    let input2 = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 0.5000 10.000px\"/>";
    let plugin = CleanupListOfValues(CleanupListOfValuesParams {
        float_precision: 3,
        leading_zero: false,
        default_px: false,
        convert_to_px: false,
    });
    let out2 = optimize_with_plugin(input2, &plugin).unwrap();
    assert!(out2.contains("viewBox=\"0 0 0.5 10px\""), "params honoured: {out2}");
}
