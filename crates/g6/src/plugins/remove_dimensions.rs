//! Port of `reference/svgo/plugins/removeDimensions.js`.

use crate::tools::js_number_to_string;
use crate::xast::{XastChild, XastElement, XastRoot};

pub const NAME: &str = "removeDimensions";
pub const DESCRIPTION: &str =
    "removes width and height in presence of viewBox (opposite to removeViewBox)";

pub fn apply(root: &mut XastRoot) {
    walk_children(&mut root.children);
}

fn walk_children(children: &mut [XastChild]) {
    for child in children.iter_mut() {
        if let XastChild::Element(el) = child {
            apply_element(el);
            walk_children(&mut el.children);
        }
    }
}

fn apply_element(el: &mut XastElement) {
    if el.name != "svg" {
        return;
    }
    if el.get_attr("viewBox").is_some() {
        el.remove_attr("width");
        el.remove_attr("height");
        return;
    }
    let (Some(width), Some(height)) = (el.get_attr("width"), el.get_attr("height")) else {
        return;
    };
    let (Some(width_num), Some(height_num)) = (js_number(width), js_number(height)) else {
        return;
    };
    let view_box = format!(
        "0 0 {} {}",
        js_dimension_to_string(width_num),
        js_dimension_to_string(height_num)
    );
    el.set_attr("viewBox", view_box);
    el.remove_attr("width");
    el.remove_attr("height");
}

/// JS `Number(string)` for dimension checks: `Some` when the result is
/// not `NaN`, `None` otherwise.
fn js_number(raw: &str) -> Option<f64> {
    let s = raw.trim();
    if s.is_empty() {
        return Some(0.0);
    }
    match s {
        "Infinity" | "+Infinity" => return Some(f64::INFINITY),
        "-Infinity" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    // Hex / binary / octal integer literals (no sign in JS string form
    // beyond the decimal case handled below; a leading sign makes these
    // NaN, which the decimal parse below reproduces as `None` only for
    // the prefixed forms, so reject signed prefixed forms explicitly).
    if !s.starts_with('+') && !s.starts_with('-') {
        for (prefix, radix) in [("0x", 16), ("0X", 16), ("0b", 2), ("0B", 2), ("0o", 8), ("0O", 8)] {
            if let Some(digits) = s.strip_prefix(prefix) {
                if digits.is_empty() {
                    return None;
                }
                return i64::from_str_radix(digits, radix).ok().map(|v| v as f64);
            }
        }
    }
    // Reject spellings Rust accepts but JS treats as NaN.
    let lower = s.to_ascii_lowercase();
    if lower == "inf" || lower == "+inf" || lower == "-inf" || lower == "nan" {
        return None;
    }
    match s.parse::<f64>() {
        Ok(v) if !v.is_nan() => Some(v),
        _ => None,
    }
}

fn js_dimension_to_string(value: f64) -> String {
    if value == f64::INFINITY {
        "Infinity".to_string()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".to_string()
    } else {
        js_number_to_string(value)
    }
}
