use std::path::PathBuf;
use svgo_g5::json::{parse_json, Json};
use svgo_g5::optimize_with_plugin;

const PLUGINS: &[&str] = &[
    "collapseGroups",
    "convertPathData",
    "convertTransform",
    "removeEmptyAttrs",
    "removeEmptyContainers",
    "mergePaths",
];

fn normalize(s: &str) -> String {
    s.trim().replace("\r\n", "\n")
}

/// Splits a fixture like test/plugins/_index.test.js does:
/// description === original @@@ expected @@@ params
fn run_fixture(name: &str, content: &str) -> Result<(), String> {
    let items: Vec<&str> = split_ws_delim(content, "===");
    let test = if items.len() == 2 { items[1] } else { items[0] };
    let parts: Vec<&str> = split_ws_delim(test, "@@@");
    let original = parts[0];
    let should = parts.get(1).copied().unwrap_or("");
    let params: Json = if parts.len() > 2 && !parts[2].trim().is_empty() {
        parse_json(parts[2].trim())
    } else {
        Json::Obj(Vec::new())
    };
    let multipass = if name == "convertTransform" { 1 } else { 2 };
    let mut last = original.to_string();
    for _ in 0..multipass {
        last = optimize_with_plugin(&last, name, &params);
        if normalize(&last) != normalize(should) {
            return Err(format!(
                "mismatch\n--- got ---\n{}\n--- expected ---\n{}",
                normalize(&last),
                normalize(should)
            ));
        }
    }
    Ok(())
}

fn split_ws_delim<'a>(s: &'a str, delim: &str) -> Vec<&'a str> {
    // JS: split(/\s*===\s*/) / split(/\s*@@@\s*/) — trim whitespace around delim
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(idx) = rest.find(delim) {
        out.push(rest[..idx].trim_end());
        rest = rest[idx + delim.len()..].trim_start();
    }
    out.push(rest);
    out
}

#[test]
fn svgo_fixtures() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../reference/svgo/test/plugins");
    let mut failures: Vec<String> = Vec::new();
    let mut total = 0usize;
    let mut passed = 0usize;
    let mut per_plugin: Vec<(String, usize, usize)> = Vec::new();
    for plugin in PLUGINS {
        let mut p_total = 0usize;
        let mut p_pass = 0usize;
        let mut entries: Vec<_> = std::fs::read_dir(&root)
            .expect("fixture dir missing")
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{}.", plugin))
            })
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            p_total += 1;
            total += 1;
            let content = std::fs::read_to_string(entry.path()).unwrap();
            match run_fixture(plugin, &content) {
                Ok(()) => {
                    p_pass += 1;
                    passed += 1;
                }
                Err(e) => {
                    failures.push(format!("{} {}: {}", plugin, entry.file_name().to_string_lossy(), e));
                }
            }
        }
        per_plugin.push((plugin.to_string(), p_pass, p_total));
    }
    for (name, p, t) in &per_plugin {
        eprintln!("{}: {}/{} fixtures passed", name, p, t);
    }
    eprintln!("TOTAL: {}/{}", passed, total);
    if !failures.is_empty() {
        panic!(
            "{} fixture(s) failed:\n\n{}",
            failures.len(),
            failures.join("\n\n")
        );
    }
}
