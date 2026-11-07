//! Integrated fixture tests: every preset-default plugin's upstream
//! fixtures (`reference/svgo/test/plugins/<plugin>.<n>.svg.txt`) run
//! through the root crate's single-plugin dispatch, mirroring
//! `test/plugins/_index.test.js` (pretty output, 2x idempotence,
//! `convertTransform` single-pass).

use std::path::PathBuf;
use svgo_rs::{optimize_with_plugin, PRESET_DEFAULT};

fn normalize(s: &str) -> String {
    s.trim().replace("\r\n", "\n")
}

fn split_ws_delim(text: &str, delim: &str) -> Vec<String> {
    text.split(delim).map(|p| p.trim().to_string()).collect()
}

#[test]
fn all_preset_fixtures() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("reference/svgo/test/plugins");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.to_string_lossy().ends_with(".svg.txt"))
        .collect();
    files.sort();
    let mut ran = 0;
    let mut failures: Vec<String> = Vec::new();
    for path in files {
        let file = path.file_name().unwrap().to_string_lossy().to_string();
        let stem = file.trim_end_matches(".svg.txt");
        let Some((name, _index)) = stem.rsplit_once('.') else { continue };
        if !PRESET_DEFAULT.contains(&name) {
            continue;
        }
        let data = std::fs::read_to_string(&path).unwrap();
        let normalized = normalize(&data);
        let items = split_ws_delim(&normalized, "===");
        let test = if items.len() == 2 { &items[1] } else { &items[0] };
        let parts = split_ws_delim(test, "@@@");
        let (original, should) = (&parts[0], &parts[1]);
        let params = parts.get(2).map(|s| s.as_str()).unwrap_or("{}");
        let multipass = if name == "convertTransform" { 1 } else { 2 };
        let mut current = original.clone();
        let mut result = String::new();
        for _ in 0..multipass {
            match optimize_with_plugin(&current, name, params) {
                Ok(out) => {
                    result = normalize(&out);
                    current = result.clone();
                }
                Err(e) => {
                    failures.push(format!("{file}: error {e}"));
                    result = String::new();
                    break;
                }
            }
        }
        if !result.is_empty() && result != *should {
            failures.push(format!("{file}:\n  got:      {}\n  expected: {}", result.replace('\n', "\\n"), should.replace('\n', "\\n")));
        }
        ran += 1;
    }
    assert!(ran > 250, "only ran {ran} fixtures");
    assert!(failures.is_empty(), "{} fixture failures:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn preset_smoke() {
    let input = "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" xmlns=\"http://www.w3.org/2000/svg\">\n<path d=\"M10.5 5C10.5 5.64117 10.9022 6.18838 11.4682 6.403C11.8174 6.48797 12.1822 6.48801 12.5315 6.4031C13.0975 6.18855 13.5 5.64127 13.5 5C13.5 4.17157 12.8284 3.5 12 3.5C11.1715 3.5 10.5 4.17157 10.5 5Z\" fill=\"#212121\"/>\n</svg>\n";
    let out = svgo_rs::optimize_preset(input).unwrap();
    assert!(out.starts_with("<svg xmlns="), "unexpected output: {out}");
    assert!(!out.ends_with('\n'), "output must not have trailing newline: {out:?}");
}
