//! Fixture tests ported from reference/svgo/test/plugins/_index.test.js:
//! each `<plugin>.<index>.svg.txt` holds `original @@@ expected [@@@ params]`,
//! optionally preceded by `description ===`. The plugin is run twice
//! (idempotence multipass, as in the JS harness) and compared after
//! trimming / newline normalisation.

use std::fs;
use std::path::PathBuf;

fn normalize(s: &str) -> String {
    s.trim().replace("\r\n", "\n")
}

fn run_fixture(plugin: &str, file: &std::path::Path) {
    let data = fs::read_to_string(file).unwrap();
    let items: Vec<String> = {
        let mut parts = Vec::new();
        let mut rest = normalize(&data);
        loop {
            match rest.find("===") {
                Some(i) => {
                    parts.push(rest[..i].trim_end().to_string());
                    rest = rest[i + 3..].trim_start().to_string();
                }
                None => {
                    parts.push(rest);
                    break;
                }
            }
        }
        parts
    };
    let test = if items.len() == 2 { items[1].clone() } else { items[0].clone() };
    // split(/\s*@@@\s*/)
    let mut segs: Vec<String> = Vec::new();
    let mut rest = test;
    loop {
        match rest.find("@@@") {
            Some(i) => {
                segs.push(rest[..i].trim_end().to_string());
                rest = rest[i + 3..].trim_start().to_string();
            }
            None => {
                segs.push(rest);
                break;
            }
        }
    }
    let original = segs[0].clone();
    let should = segs[1].clone();
    let params = if segs.len() > 2 {
        svgo_g3::Params::parse_json(&segs[2]).expect("fixture params")
    } else {
        svgo_g3::Params::default()
    };
    let mut last = original;
    for _ in 0..2 {
        last = svgo_g3::optimize_with_plugin(&last, plugin, &params)
            .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        assert_eq!(
            normalize(&last),
            normalize(&should),
            "fixture {} (plugin {plugin})",
            file.display()
        );
    }
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../reference/svgo/test/plugins")
}

fn run_all(plugin: &str, count: usize) {
    let dir = fixture_dir();
    for i in 1..=count {
        let file = dir.join(format!("{plugin}.{:02}.svg.txt", i));
        assert!(file.exists(), "missing fixture {}", file.display());
        run_fixture(plugin, &file);
    }
}

#[test]
fn cleanup_numeric_values_fixtures() {
    run_all("cleanupNumericValues", 3);
}

#[test]
fn convert_colors_fixtures() {
    run_all("convertColors", 7);
}

#[test]
fn remove_non_inheritable_group_attrs_fixtures() {
    run_all("removeNonInheritableGroupAttrs", 2);
}

#[test]
fn remove_useless_stroke_and_fill_fixtures() {
    run_all("removeUselessStrokeAndFill", 5);
}

#[test]
fn cleanup_enable_background_fixtures() {
    run_all("cleanupEnableBackground", 5);
}
