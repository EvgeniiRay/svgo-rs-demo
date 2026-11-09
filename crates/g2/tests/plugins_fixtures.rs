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

#[test]
fn minify_styles_removes_empty_at_rules() {
    // CSSO drops a rule/at-rule whose declarations or nested rules are all
    // removed (e.g. unused selectors), and drops the <style> if nothing remains.
    let input = r#"<svg xmlns="http://www.w3.org/2000/svg"><style>@media screen and (max-width:200px){.unused{display:none}}</style><rect width="100" height="100"/></svg>"#;
    let out = svgo_g2::optimize_plugin(input, "minifyStyles", "{}").unwrap();
    assert_eq!(
        normalize(&out),
        "<svg xmlns=\"http://www.w3.org/2000/svg\">\n    <rect width=\"100\" height=\"100\"/>\n</svg>"
    );

    let input = r#"<svg xmlns="http://www.w3.org/2000/svg"><style>@media print{.st0{fill:red}}@media only screen and (min-width:600px){.wrapper{color:blue}}</style><rect class="st0" width="100" height="100"/></svg>"#;
    let out = svgo_g2::optimize_plugin(input, "minifyStyles", "{}").unwrap();
    assert_eq!(
        normalize(&out),
        "<svg xmlns=\"http://www.w3.org/2000/svg\">\n    <style>\n        @media print{.st0{fill:red}}\n    </style>\n    <rect class=\"st0\" width=\"100\" height=\"100\"/>\n</svg>"
    );
}

#[test]
fn minify_styles_lowercases_and_shortens_hex_colors() {
    // CSSO (JS minifyStyles) normalizes hex colors in style attributes
    // and <style> blocks, but leaves url(#id), strings and var() alone.
    let input = r##"<svg xmlns="http://www.w3.org/2000/svg"><style>.a{fill:#231F20;background:#AABBCC}</style><rect class="a" style="fill:#231F20;stroke:url(#Foo)"/></svg>"##;
    let out = svgo_g2::optimize_plugin(input, "minifyStyles", "{}").unwrap();
    assert!(out.contains("fill:#231f20"), "style attr/element not normalized: {out}");
    assert!(out.contains("#abc"), "shortenable hex not shortened: {out}");
    assert!(out.contains("url(#Foo)"), "url fragment must be preserved: {out}");
}

#[test]
fn minify_styles_compresses_colors_like_csso() {
    // CSSO (via minifyStyles) rewrites color values in style attributes and
    // <style> rules to their shortest form: magenta -> #f0f, while red stays
    // red (shorter than #f00) and #ff0000 -> red.
    let input = "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect style=\"fill:magenta\"/><rect style=\"fill:red\"/><rect style=\"fill:#ff0000\"/></svg>";
    let out = svgo_g2::optimize_plugin(input, "minifyStyles", "{}").expect("minifyStyles");
    assert!(out.contains("fill:#f0f"), "magenta not compressed: {out}");
    assert!(out.contains("fill:red"), "red changed: {out}");
    assert!(!out.contains("#ff0000"), "long hex not compressed: {out}");
    assert!(!out.contains("magenta"), "magenta left behind: {out}");
}

plugin_test!(merge_styles_fixtures, "mergeStyles");
plugin_test!(inline_styles_fixtures, "inlineStyles");
plugin_test!(minify_styles_fixtures, "minifyStyles");
plugin_test!(cleanup_ids_fixtures, "cleanupIds");
plugin_test!(remove_useless_defs_fixtures, "removeUselessDefs");
