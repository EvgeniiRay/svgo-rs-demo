//! Ports of `reference/svgo/test/plugins/_index.test.js` for the
//! g1-cleanup plugins: same `*.svg.txt` fixtures, same
//! `input @@@ expected @@@ params` format, run twice (idempotence),
//! compared with `js2svg: { pretty: true }` output.

use std::fs;
use std::path::PathBuf;

fn normalize(s: &str) -> String {
    s.trim().replace("\r\n", "\n")
}

/// Splits on `/\s*===\s*/` then `/\s*@@@\s*/` like the JS harness.
fn split_fixture(data: &str) -> (String, String, String) {
    let normalized = normalize(data);
    let desc_split: Vec<&str> = normalized.split("===").collect();
    let test = if desc_split.len() == 2 {
        desc_split[1].trim().to_string()
    } else {
        normalized.clone()
    };
    let parts: Vec<&str> = test.split("@@@").collect();
    let original = parts.first().map(|s| s.trim().to_string()).unwrap_or_default();
    let should = parts.get(1).map(|s| s.trim().to_string()).unwrap_or_default();
    let params = parts.get(2).map(|s| s.trim().to_string()).unwrap_or_default();
    (original, should, params)
}

fn run_fixtures(plugin: &str) -> (usize, Vec<String>) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../reference/svgo/test/plugins");
    let prefix = format!("{plugin}.");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("fixture dir exists")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name().is_some_and(|n| {
                let n = n.to_string_lossy();
                n.starts_with(&prefix) && n.ends_with(".svg.txt")
            })
        })
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no fixtures found for {plugin}");
    let mut failures = Vec::new();
    for file in &files {
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let data = fs::read_to_string(file).expect("read fixture");
        let (original, should, params) = split_fixture(&data);
        let mut last = original;
        let mut result: Result<(), String> = Ok(());
        // Test plugin idempotence: run twice, like the JS harness.
        for _ in 0..2 {
            match svgo_g1::optimize_with_plugin(&last, plugin, &params) {
                Ok(out) => last = out,
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }
        match result {
            Err(e) => failures.push(format!("{name}: error: {e}")),
            Ok(()) => {
                if normalize(&last) != should {
                    failures.push(format!(
                        "{name}:\n--- expected ---\n{should}\n--- actual ---\n{}",
                        normalize(&last)
                    ));
                }
            }
        }
    }
    (files.len(), failures)
}

macro_rules! fixture_test {
    ($test:ident, $plugin:literal) => {
        #[test]
        fn $test() {
            let (count, failures) = run_fixtures($plugin);
            assert!(
                failures.is_empty(),
                "{}: {}/{} fixtures failed:\n{}",
                $plugin,
                failures.len(),
                count,
                failures.join("\n")
            );
        }
    };
}

#[test]
fn edge_cases_beyond_fixtures() {
    // removeComments with a custom preserve pattern.
    let out = svgo_g1::optimize_with_plugin(
        "<svg><!-- keep me --><!-- drop --></svg>",
        "removeComments",
        r#"{"preservePatterns":["keep"]}"#,
    )
    .unwrap();
    assert!(out.contains("keep me"), "{out}");
    assert!(!out.contains("drop"), "{out}");
    // removeComments rejects a non-array preservePatterns, like JS.
    assert!(svgo_g1::optimize_with_plugin(
        "<svg/>",
        "removeComments",
        r#"{"preservePatterns":"keep"}"#
    )
    .is_err());
    // cleanupAttrs with all steps disabled is a no-op for values.
    let out = svgo_g1::optimize_with_plugin(
        "<svg attr=\"a  b\"/>",
        "cleanupAttrs",
        r#"{"newlines":false,"trim":false,"spaces":false}"#,
    )
    .unwrap();
    assert!(out.contains("a  b"), "{out}");
    // removeEditorsNSData with an additional custom namespace.
    let out = svgo_g1::optimize_with_plugin(
        "<svg xmlns:foo=\"http://example.com/foo\"><foo:bar/><path foo:x=\"1\"/></svg>",
        "removeEditorsNSData",
        r#"{"additionalNamespaces":["http://example.com/foo"]}"#,
    )
    .unwrap();
    assert!(!out.contains("foo"), "{out}");
}

fixture_test!(remove_doctype_fixtures, "removeDoctype");
fixture_test!(remove_xml_proc_inst_fixtures, "removeXMLProcInst");
fixture_test!(remove_comments_fixtures, "removeComments");
fixture_test!(remove_metadata_fixtures, "removeMetadata");
fixture_test!(remove_editors_ns_data_fixtures, "removeEditorsNSData");
fixture_test!(cleanup_attrs_fixtures, "cleanupAttrs");
