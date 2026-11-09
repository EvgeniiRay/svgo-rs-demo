//! Port of `reference/svgo/plugins/cleanupListOfValues.js`.

use crate::tools::{js_number_to_string, remove_leading_zero, to_fixed_number};
use crate::xast::{XastChild, XastElement, XastRoot};

pub const NAME: &str = "cleanupListOfValues";
pub const DESCRIPTION: &str = "rounds list of values to the fixed precision";

#[derive(Debug, Clone, Copy)]
pub struct CleanupListOfValuesParams {
    pub float_precision: usize,
    pub leading_zero: bool,
    pub default_px: bool,
    pub convert_to_px: bool,
}

impl Default for CleanupListOfValuesParams {
    fn default() -> Self {
        Self { float_precision: 3, leading_zero: true, default_px: true, convert_to_px: true }
    }
}

const ABSOLUTE_LENGTHS: &[(&str, f64)] = &[
    ("cm", 96.0 / 2.54),
    ("mm", 96.0 / 25.4),
    ("in", 96.0),
    ("pt", 4.0 / 3.0),
    ("pc", 16.0),
    ("px", 1.0),
];
const UNITS: &[&str] = &["px", "pt", "pc", "mm", "cm", "m", "in", "ft", "em", "ex", "%"];

/// Split on JS `/\s+,?\s*|,\s*/`, dropping empty pieces exactly as
/// `roundValues` effectively does (empty pieces are never pushed).
fn split_list(lists: &str) -> Vec<String> {
    let chars: Vec<char> = lists.chars().collect();
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            // \s+,?\s*
            while i < chars.len() && chars[i].is_whitespace() { i += 1; }
            if i < chars.len() && chars[i] == ',' { i += 1; }
            while i < chars.len() && chars[i].is_whitespace() { i += 1; }
            parts.push(std::mem::take(&mut cur));
        } else if c == ',' {
            // ,\s*
            i += 1;
            while i < chars.len() && chars[i].is_whitespace() { i += 1; }
            parts.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
            i += 1;
        }
    }
    parts.push(cur);
    parts
}

/// Match JS `/^([-+]?\d*\.?\d+([eE][-+]?\d+)?)(px|pt|pc|mm|cm|m|in|ft|em|ex|%)?$/`.
/// Returns (numeric part, exponent-included numeric string, unit).
fn match_numeric(elem: &str) -> Option<(String, String)> {
    let bytes = elem.as_bytes();
    let mut i = 0;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') { i += 1; }
    let int_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
    let int_digits = i - int_start;
    let mut frac_digits = 0usize;
    let mut has_dot = false;
    if i < bytes.len() && bytes[i] == b'.' {
        // \d*\.?\d+ : a dot is only consumable if digits follow it, unless
        // digits preceded and the \d+ can be satisfied before the dot --
        // but the regex is anchored, so try both shapes below via checks.
        let after = i + 1;
        let mut j = after;
        while j < bytes.len() && bytes[j].is_ascii_digit() { j += 1; }
        if j > after {
            has_dot = true;
            frac_digits = j - after;
            i = j;
        }
    }
    // Require \d*\.?\d+ overall: at least one digit after optional dot,
    // or (int digits with no dot). "5." and "." and "" are rejected.
    let numeric_ok = if has_dot { frac_digits > 0 } else { int_digits > 0 && {
        // int part itself satisfies \d+ as the trailing \d+ only when
        // \d* can give back a digit: always true for >=1 digit.
        true
    }};
    if !numeric_ok { return None; }
    // Exponent.
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        let mut j = i + 1;
        if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') { j += 1; }
        let exp_start = j;
        while j < bytes.len() && bytes[j].is_ascii_digit() { j += 1; }
        if j > exp_start { i = j; }
        // If exponent has no digits, regex backtracks to no-exponent;
        // the leftover 'e...' then fails unit/anchor check below.
    }
    let numeric = &elem[..i];
    let unit = &elem[i..];
    if unit.is_empty() {
        return Some((numeric.to_string(), String::new()));
    }
    if UNITS.contains(&unit) {
        return Some((numeric.to_string(), unit.to_string()));
    }
    None
}

fn round_values(lists: &str, params: &CleanupListOfValuesParams) -> String {
    let mut rounded: Vec<String> = Vec::new();
    for elem in split_list(lists) {
        if let Some((numeric_str, mut units)) = match_numeric(&elem) {
            let original: f64 = numeric_str.parse().unwrap_or(0.0);
            let mut num = to_fixed_number(original, params.float_precision);
            if params.convert_to_px && !units.is_empty() {
                if let Some(&(_, factor)) = ABSOLUTE_LENGTHS.iter().find(|(u, _)| *u == units) {
                    let px_num = to_fixed_number(factor * original, params.float_precision);
                    if js_number_to_string(px_num).len() < elem.len() {
                        num = px_num;
                        units = "px".to_string();
                    }
                }
            }
            let s = if params.leading_zero {
                remove_leading_zero(num)
            } else {
                js_number_to_string(num)
            };
            if params.default_px && units == "px" {
                units = String::new();
            }
            rounded.push(format!("{s}{units}"));
        } else if elem.contains("new") {
            rounded.push("new".to_string());
        } else if !elem.is_empty() {
            rounded.push(elem);
        }
    }
    rounded.join(" ")
}

const ATTRS: &[&str] = &["points", "enable-background", "viewBox", "stroke-dasharray", "dx", "dy", "x", "y"];

fn visit(el: &mut XastElement, params: &CleanupListOfValuesParams) {
    for attr in ATTRS {
        if let Some(value) = el.get_attr(attr).map(|v| v.to_string()) {
            let rounded = round_values(&value, params);
            el.set_attr(attr, rounded);
        }
    }
    for child in el.children.iter_mut() {
        if let XastChild::Element(child_el) = child {
            visit(child_el, params);
        }
    }
}

pub fn apply(root: &mut XastRoot, params: &CleanupListOfValuesParams) {
    for child in root.children.iter_mut() {
        if let XastChild::Element(el) = child {
            visit(el, params);
        }
    }
}
