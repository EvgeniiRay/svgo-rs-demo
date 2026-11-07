//! Fixture tests ported from SVGO's `test/plugins/_index.test.js` for this
//! group's plugins. Fixture files are read directly from
//! `reference/svgo/test/plugins/` and run with multipass = 2 (idempotence),
//! exactly like the JS harness.

use std::path::PathBuf;

fn split_ws_marker(s: &str, marker: &str) -> Vec<String> {
    // Split on /\s*@@@\s*/ or /\s*===\s*/ like JS String.split with regex.
    let mut parts = Vec::new();
    let mut rest = s;
    loop {
        match rest.find(marker) {
            None => {
                parts.push(rest.to_string());
                break;
            }
            Some(pos) => {
                let before_full = &rest[..pos];
                let before = before_full.trim_end();
                // start after marker, skipping following whitespace incl newlines
                let after_full = &rest[pos + marker.len()..];
                let after = after_full.trim_start();
                // JS regex \s*@@@\s* also consumes the whitespace we trimmed
                parts.push(before.to_string());
                rest = after;
            }
        }
    }
    parts
}

fn normalize(s: &str) -> String {
    s.trim().replace("\r\n", "\n")
}

struct Case {
    plugin: String,
    index: String,
    original: String,
    should: String,
    params: String,
}

fn load_cases() -> Vec<Case> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../reference/svgo/test/plugins");
    let plugins = ["mergeStyles", "inlineStyles", "minifyStyles", "cleanupIds", "removeUselessDefs"];
    let mut cases = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("fixture dir")
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let plugin = match plugins.iter().find(|p| name.starts_with(&format!("{p}."))) {
            Some(p) => p.to_string(),
            None => continue,
        };
        if !name.ends_with(".svg.txt") {
            continue;
        }
        let index = name
            .trim_start_matches(&format!("{plugin}."))
            .trim_end_matches(".svg.txt")
            .to_string();
        let data = std::fs::read_to_string(entry.path()).expect("read fixture");
        let data = normalize(&data);
        let items = split_ws_marker(&data, "===");
        let test = if items.len() == 2 { items[1].clone() } else { items[0].clone() };
        let parts = split_ws_marker(&test, "@@@");
        let original = parts.get(0).cloned().unwrap_or_default();
        let should = parts.get(1).cloned().unwrap_or_default();
        let params = parts.get(2).cloned().unwrap_or_else(|| "{}".to_string());
        let params = if params.trim().is_empty() { "{}".to_string() } else { params };
        cases.push(Case { plugin, index, original, should, params });
    }
    cases
}

fn run_case(case: &Case) -> Result<(), String> {
    let mut last = case.original.clone();
    for _ in 0..2 {
        let out = svgo_g2::optimize_plugin(&last, &case.plugin, &case.params)
            .ok_or_else(|| format!("unknown plugin {}", case.plugin))?;
        last = out;
        let got = normalize(&last);
        if got != case.should {
            return Err(format!(
                "mismatch\n--- expected ---\n{}\n--- got ---\n{}",
                case.should, got
            ));
        }
    }
    Ok(())
}

#[test]
fn all_fixtures() {
    let cases = load_cases();
    assert!(!cases.is_empty(), "no fixtures loaded");
    let mut failures: Vec<String> = Vec::new();
    let mut per_plugin: std::collections::BTreeMap<String, (usize, usize)> =
        std::collections::BTreeMap::new();
    for case in &cases {
        let entry = per_plugin.entry(case.plugin.clone()).or_insert((0, 0));
        match run_case(case) {
            Ok(()) => entry.0 += 1,
            Err(e) => {
                entry.1 += 1;
                failures.push(format!("{}.{}: {}", case.plugin, case.index, e));
            }
        }
    }
    for (plugin, (pass, fail)) in &per_plugin {
        eprintln!("{plugin}: {pass} passed, {fail} failed");
    }
    if !failures.is_empty() {
        panic!("{} fixture failures:\n\n{}", failures.len(), failures.join("\n\n"));
    }
}

// Individual tests for clearer reporting
macro_rules! plugin_test {
    ($name:ident, $plugin:expr) => {
        #[test]
        fn $name() {
            let cases: Vec<_> = load_cases().into_iter().filter(|c| c.plugin == $plugin).collect();
            assert!(!cases.is_empty(), "no fixtures for {}", $plugin);
            let mut failures = Vec::new();
            for case in &cases {
                if let Err(e) = run_case(case) {
                    failures.push(format!("{}.{}: {}", case.plugin, case.index, e));
                }
            }
            assert!(
                failures.is_empty(),
                "{} {} failures:\n\n{}",
                $plugin,
                failures.len(),
                failures.join("\n\n")
            );
        }
    };
}

plugin_test!(merge_styles_fixtures, "mergeStyles");
plugin_test!(inline_styles_fixtures, "inlineStyles");
plugin_test!(minify_styles_fixtures, "minifyStyles");
plugin_test!(cleanup_ids_fixtures, "cleanupIds");
plugin_test!(remove_useless_defs_fixtures, "removeUselessDefs");
