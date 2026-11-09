//! Port of plugins/cleanupNumericValues.js

use crate::tools::{js_num_to_string, js_to_fixed_number, remove_leading_zero};
use crate::xast::{walk_elements, Root};
use crate::Params;

#[derive(Debug, Clone)]
pub struct CleanupNumericValuesParams {
    pub float_precision: usize,
    pub leading_zero: bool,
    pub default_px: bool,
    pub convert_to_px: bool,
}
impl Default for CleanupNumericValuesParams {
    fn default() -> Self {
        Self { float_precision: 3, leading_zero: true, default_px: true, convert_to_px: true }
    }
}
impl From<&Params> for CleanupNumericValuesParams {
    fn from(p: &Params) -> Self {
        let mut d = Self::default();
        if let Some(v) = p.num("floatPrecision") { d.float_precision = v as usize; }
        if let Some(v) = p.bool("leadingZero") { d.leading_zero = v; }
        if let Some(v) = p.bool("defaultPx") { d.default_px = v; }
        if let Some(v) = p.bool("convertToPx") { d.convert_to_px = v; }
        d
    }
}

const ABSOLUTE: &[(&str, f64)] = &[
    ("cm", 96.0 / 2.54),
    ("mm", 96.0 / 25.4),
    ("in", 96.0),
    ("pt", 4.0 / 3.0),
    ("pc", 16.0),
    ("px", 1.0),
];
const UNITS: &[&str] = &["px", "pt", "pc", "mm", "cm", "in", "ft", "em", "ex", "%", "m"];

/// Matches /^([-+]?\d*\.?\d+([eE][-+]?\d+)?)(px|pt|pc|mm|cm|m|in|ft|em|ex|%)?$/
/// Returns (number text, unit).
fn match_numeric(value: &str) -> Option<(&str, &str)> {
    let (num, unit) = match UNITS.iter().find(|u| value.ends_with(**u)) {
        Some(u) => (&value[..value.len() - u.len()], *u),
        None => (value, ""),
    };
    if num.is_empty() {
        return None;
    }
    // validate number: [-+]? \d*\.?\d+ ([eE][-+]?\d+)?
    let b = num.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') { i += 1; }
    let mut digits = 0;
    while i < b.len() && b[i].is_ascii_digit() { i += 1; digits += 1; }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() { i += 1; digits += 1; }
    }
    if digits == 0 {
        return None;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') { i += 1; }
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() { i += 1; }
        if i == start { return None; }
    }
    if i != b.len() {
        return None;
    }
    Some((num, unit))
}

pub fn apply(root: &mut Root, params: &CleanupNumericValuesParams) {
    let prec = params.float_precision;
    walk_elements(&mut root.children, &mut |node| {
        if let Some(view_box) = node.get("viewBox").map(|s| s.to_string()) {
            let parts: Vec<String> = view_box
                .trim()
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|s| !s.is_empty())
                .map(|value| match value.parse::<f64>() {
                    Ok(num) => js_num_to_string(js_to_fixed_number(num, prec)),
                    Err(_) => value.to_string(),
                })
                .collect();
            node.set("viewBox", &parts.join(" "));
        }
        for name in node.attr_names() {
            if name == "version" {
                continue;
            }
            let value = match node.get(&name) { Some(v) => v.to_string(), None => continue };
            if let Some((num_text, unit0)) = match_numeric(&value) {
                let orig_num: f64 = match num_text.parse() { Ok(n) => n, Err(_) => continue };
                let mut num = js_to_fixed_number(orig_num, prec);
                let mut units = unit0.to_string();
                if params.convert_to_px && !units.is_empty() {
                    if let Some((_, factor)) = ABSOLUTE.iter().find(|(u, _)| *u == units) {
                        let px_num = js_to_fixed_number(factor * orig_num, prec);
                        if js_num_to_string(px_num).len() < value.len() {
                            num = px_num;
                            units = "px".to_string();
                        }
                    }
                }
                let mut s = if params.leading_zero { remove_leading_zero(num) } else { js_num_to_string(num) };
                if params.default_px && units == "px" {
                    units.clear();
                }
                s.push_str(&units);
                node.set(&name, &s);
            }
        }
    });
}
