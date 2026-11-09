//! Port of `plugins/cleanupAttrs.js`.
//! JS regexes: regNewlinesNeedSpace `/(\S)\r?\n(\S)/g`,
//! regNewlines `/\r?\n/g`, regSpaces `/\s{2,}/g`; params
//! `newlines`/`trim`/`spaces` all default to true.

use crate::json::JsonValue;
use crate::xast::{XastChild, XastRoot};

pub fn apply(root: &mut XastRoot, params: &JsonValue) -> Result<(), String> {
    let flag = |key: &str| params.get(key).and_then(|v| v.as_bool()).unwrap_or(true);
    let (newlines, trim, spaces) = (flag("newlines"), flag("trim"), flag("spaces"));
    visit(&mut root.children, newlines, trim, spaces);
    Ok(())
}

fn visit(children: &mut [XastChild], newlines: bool, trim: bool, spaces: bool) {
    for child in children.iter_mut() {
        if let XastChild::Element(el) = child {
            for (_, value) in el.attributes.iter_mut() {
                if newlines {
                    *value = replace_newlines_need_space(value);
                    *value = value.replace("\r\n", "").replace('\n', "");
                }
                if trim {
                    *value = value.trim().to_string();
                }
                if spaces {
                    *value = collapse_spaces(value);
                }
            }
            visit(&mut el.children, newlines, trim, spaces);
        }
    }
}

/// `/(\S)\r?\n(\S)/g` -> `$1 $2`, with JS global-regex consumption:
/// the second capture is consumed, so it cannot start the next match.
fn replace_newlines_need_space(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        let is_match = !chars[i].is_whitespace()
            && ((i + 2 < chars.len()
                && chars[i + 1] == '\n'
                && !chars[i + 2].is_whitespace())
                || (i + 3 < chars.len()
                    && chars[i + 1] == '\r'
                    && chars[i + 2] == '\n'
                    && !chars[i + 3].is_whitespace()));
        if is_match {
            let nl_len = if chars[i + 1] == '\r' { 2 } else { 1 };
            let p2 = chars[i + 1 + nl_len];
            out.push(chars[i]);
            out.push(' ');
            out.push(p2);
            i += 2 + nl_len;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// `/\s{2,}/g` -> `" "`: runs of 2+ whitespace become one space;
/// a single whitespace char is kept verbatim. JS `\s` is approximated
/// by Unicode whitespace.
fn collapse_spaces(value: &str) -> String {
    let mut out = String::new();
    let mut run: Vec<char> = Vec::new();
    let flush = |run: &Vec<char>, out: &mut String| {
        if run.len() >= 2 {
            out.push(' ');
        } else if let Some(c) = run.first() {
            out.push(*c);
        }
    };
    for c in value.chars() {
        if c.is_whitespace() {
            run.push(c);
        } else {
            flush(&run, &mut out);
            run.clear();
            out.push(c);
        }
    }
    flush(&run, &mut out);
    out
}
