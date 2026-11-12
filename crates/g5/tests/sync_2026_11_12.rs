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

// svg/svgo@31194fb: when a cubic is lowered to a quadratic, a following
// s must reflect the cubic's second control point, not the quadratic's.
// Upstream convertPathData.43.
#[test]
fn convert_path_data_43_reflects_cubic_control_point_into_s() {
    run(
        "convertPathData",
        "<svg xmlns=\"http://www.w3.org/2000/svg\">\n    <path d=\"M0 6c0 4 2 6 6 6s6 3 6 6\"/>\n    <path d=\"M0 6C0 10 2 12 6 12S12 15 12 18\"/>\n</svg>",
        "{}",
        "<svg xmlns=\"http://www.w3.org/2000/svg\">\n    <path d=\"M0 6q0 6 6 6c4 0 6 3 6 6\"/>\n    <path d=\"M0 6q0 6 6 6c4 0 6 3 6 6\"/>\n</svg>",
    );
}

// svg/svgo@31194fb: upstream convertPathData.16 expected output changed
// for the same reason (the s after the lowered cubic reflects (11,2)
// around (9,2), giving (7,2), not the quadratic's (13,2) giving (5,2)).
#[test]
fn convert_path_data_16_reflects_cubic_control_point_into_s() {
    run(
        "convertPathData",
        "<svg xmlns=\"http://www.w3.org/2000/svg\">\n    <path d=\"M.49 8.8c-.3-.75-.44-1.55-.44-2.35 0-3.54 2.88-6.43 6.43-6.43 3.53 0 6.42 2.88 6.42 6.43 0 .8-.15 1.6-.43 2.35\"/>\n    <path d=\"M13.4 6.62c0-2.5-1.98-4.57-4.4-4.57S4.6 4.1 4.6 6.62\"/>\n</svg>",
        "{ \"floatPrecision\": 0 }",
        "<svg xmlns=\"http://www.w3.org/2000/svg\">\n    <path d=\"M0 9V6a6 6 0 1 1 12 3\"/>\n    <path d=\"M13 7q0-5-4-5-3 0-4 5\"/>\n</svg>",
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
