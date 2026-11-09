//! Integrated opt-in tests for `removeDimensions`, ported from
//! `reference/svgo/test/plugins/removeDimensions.*.svg.txt`, mirroring
//! `tests/preset_fixtures.rs` (pretty output, 2x idempotence).

use std::path::PathBuf;
use svgo_rs::{optimize_with_plugin, PRESET_DEFAULT};

fn normalize(s: &str) -> String {
    s.trim().replace("\r\n", "\n")
}

fn split_ws_delim(text: &str, delim: &str) -> Vec<String> {
    text.split(delim).map(|p| p.trim().to_string()).collect()
}

#[test]
fn remove_dimensions_fixtures() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("reference/svgo/test/plugins");
    for index in ["01", "02", "03", "04", "05"] {
        let path = dir.join(format!("removeDimensions.{index}.svg.txt"));
        let data = std::fs::read_to_string(&path).unwrap();
        let normalized = normalize(&data);
        let items = split_ws_delim(&normalized, "===");
        let test = if items.len() == 2 { &items[1] } else { &items[0] };
        let parts = split_ws_delim(test, "@@@");
        let (original, should) = (&parts[0], &parts[1]);
        let params = parts.get(2).map(|s| s.as_str()).unwrap_or("{}");
        let mut current = original.clone();
        for pass in 0..2 {
            let out = optimize_with_plugin(&current, "removeDimensions", params)
                .unwrap_or_else(|e| panic!("removeDimensions.{index} pass {pass}: {e}"));
            let result = normalize(&out);
            assert_eq!(result, normalize(should), "removeDimensions.{index} pass {pass}");
            current = result;
        }
    }
}

#[test]
fn remove_dimensions_is_opt_in() {
    assert!(
        !PRESET_DEFAULT.contains(&"removeDimensions"),
        "removeDimensions must stay out of preset-default"
    );
}
