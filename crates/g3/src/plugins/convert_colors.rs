//! Port of plugins/convertColors.js

use crate::collections::{COLORS_NAMES, COLORS_PROPS, COLORS_SHORT_NAMES};
use crate::tools::{includes_css_var_reference, includes_url_reference};
use crate::xast::{Child, Element, Root};
use crate::{ParamValue, Params};

#[derive(Debug, Clone, PartialEq)]
pub enum CurrentColor {
    Bool(bool),
    Str(String),
    /// Regex source; matched as a search (see unresolved note on regex support).
    Regex(String),
}

#[derive(Debug, Clone)]
pub struct ConvertColorsParams {
    pub current_color: CurrentColor,
    pub names2hex: bool,
    pub rgb2hex: bool,
    pub convert_case: Option<String>, // None = false, else "lower"/"upper"
    pub shorthex: bool,
    pub shortname: bool,
}
impl Default for ConvertColorsParams {
    fn default() -> Self {
        Self {
            current_color: CurrentColor::Bool(false),
            names2hex: true,
            rgb2hex: true,
            convert_case: Some("lower".into()),
            shorthex: true,
            shortname: true,
        }
    }
}
impl From<&Params> for ConvertColorsParams {
    fn from(p: &Params) -> Self {
        let mut d = Self::default();
        match p.get("currentColor") {
            Some(ParamValue::Bool(b)) => d.current_color = CurrentColor::Bool(*b),
            Some(ParamValue::Str(s)) => d.current_color = CurrentColor::Str(s.clone()),
            _ => {}
        }
        if let Some(v) = p.bool("names2hex") { d.names2hex = v; }
        if let Some(v) = p.bool("rgb2hex") { d.rgb2hex = v; }
        match p.get("convertCase") {
            Some(ParamValue::Bool(false)) => d.convert_case = None,
            Some(ParamValue::Str(s)) => d.convert_case = Some(s.clone()),
            _ => {}
        }
        if let Some(v) = p.bool("shorthex") { d.shorthex = v; }
        if let Some(v) = p.bool("shortname") { d.shortname = v; }
        d
    }
}

fn lookup<'a>(table: &'a [(&'a str, &'a str)], key: &str) -> Option<&'a str> {
    table.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

fn parse_rgb(val: &str) -> Option<[f64; 3]> {
    let inner = val.strip_prefix("rgb(")?.strip_suffix(')')?;
    // separators are (?:\s*,\s*|\s+); tokenise on commas/whitespace
    let toks: Vec<&str> = inner.split(|c: char| c == ',' || c.is_whitespace()).filter(|s| !s.is_empty()).collect();
    if toks.len() != 3 {
        return None;
    }
    // validate separator shape roughly: either commas present or spaces
    let mut out = [0f64; 3];
    for (i, t) in toks.iter().enumerate() {
        let (num, pct) = match t.strip_suffix('%') {
            Some(n) => (n, true),
            None => (*t, false),
        };
        // rNumber: [+-]?(?:\d*\.\d+|\d+\.?)
        let n: f64 = num.parse().ok()?;
        let v = if pct { (n * 2.55 + 0.5).floor() } else { n }; // Math.round
        out[i] = v.clamp(0.0, 255.0);
    }
    Some(out)
}

fn rgb_to_hex(rgb: [f64; 3]) -> String {
    // JS bit ops truncate each component toward zero via ToInt32
    format!("#{:02X}{:02X}{:02X}", rgb[0] as i64, rgb[1] as i64, rgb[2] as i64)
}

fn short_hex(val: &str) -> Option<String> {
    let b = val.as_bytes();
    if b.len() != 7 || b[0] != b'#' {
        return None;
    }
    let hex = |c: u8| (c as char).is_ascii_hexdigit();
    if hex(b[1]) && b[1] == b[2] && hex(b[3]) && b[3] == b[4] && hex(b[5]) {
        Some(format!("#{}{}{}", b[1] as char, b[3] as char, b[5] as char))
    } else {
        None
    }
}

pub fn apply(root: &mut Root, params: &ConvertColorsParams) {
    let mut mask_counter: i32 = 0;
    fn enter(node: &mut Element, params: &ConvertColorsParams, mask_counter: &mut i32) {
        if node.name == "mask" {
            *mask_counter += 1;
        }
        for name in node.attr_names() {
            if !COLORS_PROPS.contains(&name.as_str()) {
                continue;
            }
            let mut val = node.get(&name).unwrap_or("").to_string();
            // currentColor
            let cc_enabled = !matches!(&params.current_color, CurrentColor::Bool(false));
            if cc_enabled && *mask_counter == 0 {
                let matched = match &params.current_color {
                    CurrentColor::Str(s) => &val == s,
                    CurrentColor::Regex(pat) => val.contains(pat.as_str()),
                    CurrentColor::Bool(_) => val != "none",
                };
                if matched {
                    val = "currentColor".to_string();
                }
            }
            if params.names2hex {
                let key = val.to_lowercase();
                if let Some(hex) = lookup(COLORS_NAMES, &key) {
                    val = hex.to_string();
                }
            }
            if params.rgb2hex {
                if let Some(rgb) = parse_rgb(&val) {
                    val = rgb_to_hex(rgb);
                }
            }
            if let Some(case) = &params.convert_case {
                if !includes_url_reference(&val) && !includes_css_var_reference(&val) && val != "currentColor" {
                    val = if case == "lower" { val.to_lowercase() } else if case == "upper" { val.to_uppercase() } else { val };
                }
            }
            if params.shorthex {
                if let Some(s) = short_hex(&val) {
                    val = s;
                }
            }
            if params.shortname {
                let key = val.to_lowercase();
                if let Some(name) = lookup(COLORS_SHORT_NAMES, &key) {
                    val = name.to_string();
                }
            }
            node.set(&name, &val);
        }
    }
    fn walk(children: &mut [Child], params: &ConvertColorsParams, mask_counter: &mut i32) {
        for child in children {
            if let Child::Element(el) = child {
                enter(el, params, mask_counter);
                walk(&mut el.children, params, mask_counter);
                if el.name == "mask" {
                    *mask_counter -= 1;
                }
            }
        }
    }
    walk(&mut root.children, params, &mut mask_counter);
}
