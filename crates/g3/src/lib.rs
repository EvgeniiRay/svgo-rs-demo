//! Rust port of a subset of SVGO 4.1.0 plugins (numeric values, colors,
//! non-inheritable group attrs, useless stroke/fill, enable-background),
//! following the JS architecture: XAST DOM, plugin visitors over elements,
//! and a pretty stringifier compatible with `js2svg: { pretty: true }`.

pub mod collections;
pub mod parser;
pub mod plugins;
pub mod stringifier;
pub mod tools;
pub mod xast;

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum ParamValue {
    Bool(bool),
    Str(String),
    Num(f64),
}

/// Flat plugin params, mirroring the JSON params in SVGO fixture files.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Params(pub BTreeMap<String, ParamValue>);

impl Params {
    pub fn get(&self, key: &str) -> Option<&ParamValue> {
        self.0.get(key)
    }
    pub fn bool(&self, key: &str) -> Option<bool> {
        match self.0.get(key) {
            Some(ParamValue::Bool(b)) => Some(*b),
            _ => None,
        }
    }
    pub fn num(&self, key: &str) -> Option<f64> {
        match self.0.get(key) {
            Some(ParamValue::Num(n)) => Some(*n),
            _ => None,
        }
    }

    /// Minimal parser for the flat JSON objects used as fixture params,
    /// e.g. `{ "currentColor": true }` or `{ "shorthex": false }`.
    pub fn parse_json(input: &str) -> Result<Params, String> {
        let s = input.trim();
        let inner = s.strip_prefix('{').and_then(|t| t.strip_suffix('}')).ok_or("expected object")?.trim();
        let mut map = BTreeMap::new();
        if inner.is_empty() {
            return Ok(Params(map));
        }
        for part in split_top_level(inner) {
            let (k, v) = part.split_once(':').ok_or("expected key:value")?;
            let key = parse_json_string(k.trim())?;
            let v = v.trim();
            let value = if v == "true" {
                ParamValue::Bool(true)
            } else if v == "false" {
                ParamValue::Bool(false)
            } else if v == "null" {
                continue;
            } else if v.starts_with('"') {
                ParamValue::Str(parse_json_string(v)?)
            } else {
                ParamValue::Num(v.parse().map_err(|_| "bad number")?)
            };
            map.insert(key, value);
        }
        Ok(Params(map))
    }
}

fn split_top_level(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    let mut escaped = false;
    for c in s.chars() {
        if in_str {
            cur.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
        } else if c == '"' {
            in_str = true;
            cur.push(c);
        } else if c == ',' {
            out.push(cur.trim().to_string());
            cur = String::new();
        } else {
            cur.push(c);
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

fn parse_json_string(s: &str) -> Result<String, String> {
    let inner = s.strip_prefix('"').and_then(|t| t.strip_suffix('"')).ok_or("expected string")?;
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some(other) => out.push(other),
                None => return Err("bad escape".into()),
            }
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

/// Apply one builtin plugin by name, mirroring `optimize` with a single
/// plugin and `js2svg: { pretty: true }`.
pub fn optimize_with_plugin(input: &str, name: &str, params: &Params) -> Result<String, String> {
    let mut root = parser::parse_svg(input)?;
    match name {
        "cleanupNumericValues" => {
            plugins::cleanup_numeric_values::apply(&mut root, &params.into());
        }
        "convertColors" => {
            plugins::convert_colors::apply(&mut root, &params.into());
        }
        "removeNonInheritableGroupAttrs" => {
            plugins::remove_non_inheritable_group_attrs::apply(&mut root);
        }
        "removeUselessStrokeAndFill" => {
            plugins::remove_useless_stroke_and_fill::apply(&mut root, &params.into());
        }
        "cleanupEnableBackground" => {
            plugins::cleanup_enable_background::apply(&mut root);
        }
        _ => return Err(format!("unknown plugin {name}")),
    }
    Ok(stringifier::stringify_svg(&root))
}
