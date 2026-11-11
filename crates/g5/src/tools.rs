// Shared numeric helpers (port of lib/svgo/tools.js).
// SHARED-HELPER note: see submit unresolved list.

/// JS `toFixed` helper from lib/svgo/tools.js: `Math.round(num * pow) / pow`.
/// Note this is NOT `Number.prototype.toFixed` (which is decimal-exact);
/// SVGO's helper rounds half-up via Math.round (floor(x + 0.5)).
pub fn to_fixed(num: f64, precision: i32) -> f64 {
    let pow = 10f64.powi(precision);
    (num * pow + 0.5).floor() / pow
}

/// JS `Math.round`.
pub fn js_round(num: f64) -> f64 {
    (num + 0.5).floor()
}

/// JS number -> string (shortest form, `0.5`, `10`, `-0` -> `0`).
pub fn fmt_num(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    if !value.is_finite() {
        return value.to_string();
    }
    let av = value.abs();
    if av >= 1e21 || av < 1e-6 {
        // JS exponential: 1e+21, 1.5e-7
        let s = format!("{:e}", value);
        if let Some((m, e)) = s.split_once('e') {
            let exp: i64 = e.parse().unwrap_or(0);
            let sign = if exp >= 0 { "+" } else { "-" };
            return format!("{}e{}{}", m, sign, exp.abs());
        }
        return s;
    }
    if value == value.trunc() && av < 1e15 {
        return format!("{}", value as i64);
    }
    let s = format!("{}", value);
    s
}

/// Port of `removeLeadingZero`.
pub fn remove_leading_zero(value: f64) -> String {
    let s = fmt_num(value);
    if value > 0.0 && value < 1.0 && s.starts_with('0') {
        return s[1..].to_string();
    }
    if value > -1.0 && value < 0.0 && s.len() > 1 && s.as_bytes()[1] == b'0' {
        return format!("{}{}", &s[..1], &s[2..]);
    }
    s
}

pub struct CleanupParams {
    pub leading_zero: bool,
    pub negative_extra_space: bool,
    pub no_space_after_flags: bool,
}

/// Port of `cleanupOutData` (lib/svgo/tools.js).
pub fn cleanup_out_data(data: &[f64], params: &CleanupParams, command: Option<char>) -> String {
    let mut str_out = String::new();
    let mut prev: Option<f64> = None;
    for (i, item) in data.iter().enumerate() {
        let mut delimiter = " ";
        if i == 0 {
            delimiter = "";
        }
        if params.no_space_after_flags && (command == Some('A') || command == Some('a')) {
            let pos = i % 7;
            if pos == 4 || pos == 5 {
                delimiter = "";
            }
        }
        let item_str = if params.leading_zero {
            remove_leading_zero(*item)
        } else {
            fmt_num(*item)
        };
        if params.negative_extra_space
            && !delimiter.is_empty()
            && (*item < 0.0
                || (item_str.starts_with('.') && prev.is_some_and(|p| p % 1.0 != 0.0)))
        {
            delimiter = "";
        }
        prev = Some(*item);
        str_out.push_str(delimiter);
        str_out.push_str(&item_str);
    }
    str_out
}

pub fn includes_url_reference(body: &str) -> bool {
    // \burl\(("')?#(.+?)\1\)
    body.contains("url(#") || body.contains("url('#") || body.contains("url(\"#")
}

/// Extract `#id` references like findReferences for url(...) and href="#...".
pub fn find_references(attr: &str, value: &str) -> Vec<String> {
    let mut out = Vec::new();
    const REF_PROPS: &[&str] = &[
        "fill", "stroke", "filter", "clip-path", "mask", "marker", "marker-start",
        "marker-mid", "marker-end",
    ];
    if REF_PROPS.contains(&attr) {
        let mut rest = value;
        while let Some(idx) = rest.find("url(") {
            let after = &rest[idx + 4..];
            let after = after.trim_start_matches(['"', '\'']);
            if let Some(hash) = after.strip_prefix('#') {
                let id: String = hash.chars().take_while(|c| *c != ')' && *c != '"' && *c != '\'').collect();
                out.push(id);
            }
            rest = &rest[idx + 4..];
        }
    }
    if attr == "href" || attr.ends_with(":href") {
        if let Some(id) = value.strip_prefix('#') {
            out.push(id.to_string());
        }
    }
    if attr == "begin" {
        if let Some(dot) = value.find('.') {
            out.push(value[..dot].to_string());
        }
    }
    out
}
