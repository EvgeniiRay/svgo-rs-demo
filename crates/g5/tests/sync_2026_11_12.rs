//! Regression tests for the 2026-11-12 upstream sync.
//! Each test mirrors an upstream fixture (`reference/svgo/test/plugins/*.svg.txt`)
//! added by the synced commit, with the same input, params, and expected
//! output. The fixture files themselves are also exercised by
//! `tests/fixtures.rs` once the reference is moved.

use svgo_g5::json::{parse_json, Json};
use svgo_g5::optimize_with_plugin;

fn normalize(s: &str) -> String {
    s.trim().replace("\r\n", "\n")
}

/// Run a plugin twice, as the fixture harness does, and compare.
fn run(plugin: &str, input: &str, params: &str, expected: &str) {
    let params: Json = if params.trim().is_empty() {
        Json::Obj(Vec::new())
    } else {
        parse_json(params.trim())
    };
    let mut current = input.to_string();
    for _ in 0..2 {
        current = optimize_with_plugin(&current, plugin, &params);
    }
    assert_eq!(normalize(&current), normalize(expected));
}

// svg/svgo@7b0c686: js2path must not drop a moveto that is followed by
// another moveto when collapseRepeated is disabled; the relative moveto
// after it is measured from its point. Upstream convertPathData.41.
#[test]
fn convert_path_data_41_keeps_movetos_when_collapse_repeated_disabled() {
    run(
        "convertPathData",
        "<svg xmlns=\"http://www.w3.org/2000/svg\">\n    <path d=\"M0 0h10M20 20m5 5h1\"/>\n</svg>",
        "{ \"collapseRepeated\": false }",
        "<svg xmlns=\"http://www.w3.org/2000/svg\">\n    <path d=\"M0 0h10m10 20m5 5h1\"/>\n</svg>",
    );
}

// svg/svgo@7b0c686: movetos are marker vertices, so they are kept when
// the path has a mid marker. Upstream convertPathData.42.
#[test]
fn convert_path_data_42_keeps_movetos_with_marker_mid() {
    run(
        "convertPathData",
        "<svg xmlns=\"http://www.w3.org/2000/svg\">\n    <path d=\"M10 10M50 50h20\" marker-mid=\"url(#marker)\"/>\n</svg>",
        "{}",
        "<svg xmlns=\"http://www.w3.org/2000/svg\">\n    <path d=\"M10 10m40 40h20\" marker-mid=\"url(#marker)\"/>\n</svg>",
    );
}

// svg/svgo@7b0c686: merging must keep the position of a relative moveto
// inside the first path. Upstream mergePaths.13.
#[test]
fn merge_paths_13_keeps_relative_moveto_position() {
    run(
        "mergePaths",
        "<svg xmlns=\"http://www.w3.org/2000/svg\">\n    <path d=\"M10 10m5 5h1\"/>\n    <path d=\"M30 30h1\"/>\n</svg>",
        "{}",
        "<svg xmlns=\"http://www.w3.org/2000/svg\">\n    <path d=\"M10 10m5 5h1M30 30h1\"/>\n</svg>",
    );
}
