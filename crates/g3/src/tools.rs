//! Port of the lib/svgo/tools.js helpers used by this plugin group.

use crate::xast::Element;

/// JS `Number.prototype.toString` for the values handled here.
pub fn js_num_to_string(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    if value.fract() == 0.0 && value.abs() < 1e21 {
        return format!("{}", value as i64);
    }
    format!("{}", value)
}

/// JS `Number(value.toFixed(precision))`.
pub fn js_to_fixed_number(value: f64, precision: usize) -> f64 {
    let s = format!("{:.*}", precision, value);
    s.parse::<f64>().unwrap_or(value)
}

/// Port of `removeLeadingZero`.
pub fn remove_leading_zero(value: f64) -> String {
    let s = js_num_to_string(value);
    if value > 0.0 && value < 1.0 && s.starts_with('0') {
        return s[1..].to_string();
    }
    if value > -1.0 && value < 0.0 && s.as_bytes().get(1) == Some(&b'0') {
        return format!("{}{}", &s[..1], &s[2..]);
    }
    s
}

/// Port of `includesUrlReference`: /\burl\((["'])?#(.+?)\1\)/
pub fn includes_url_reference(body: &str) -> bool {
    let bytes = body.as_bytes();
    let mut i = 0;
    while i + 4 <= body.len() {
        if &body[i..i + 3] == "url" {
            let boundary = i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
            if boundary {
                let mut j = i + 3;
                if j < body.len() && bytes[j] == b'(' {
                    j += 1;
                    let quote = if j < body.len() && (bytes[j] == b'"' || bytes[j] == b'\'') {
                        let q = bytes[j];
                        j += 1;
                        Some(q)
                    } else {
                        None
                    };
                    if j < body.len() && bytes[j] == b'#' {
                        // need closing matching quote + ')'
                        if let Some(q) = quote {
                            if let Some(end) = body[j..].find(q as char) {
                                if body[j + end + 1..].starts_with(')') {
                                    return true;
                                }
                            }
                        } else if body[j..].contains(')') {
                            return true;
                        }
                    }
                }
            }
        }
        i += 1;
    }
    false
}

/// Port of `includesCssVarReference`: /var\s*\(\s*--/
pub fn includes_css_var_reference(body: &str) -> bool {
    let mut rest = body;
    while let Some(i) = rest.find("var") {
        let after = &rest[i + 3..];
        let after = after.trim_start();
        if let Some(after) = after.strip_prefix('(') {
            if after.trim_start().starts_with("--") {
                return true;
            }
        }
        rest = &rest[i + 3..];
    }
    false
}

const EVENT_ATTRS: &[&str] = &[
    "onbegin", "onend", "onrepeat", "onload", "onabort", "onerror", "onresize", "onscroll",
    "onunload", "onzoom", "oncopy", "oncut", "onpaste", "oncancel", "oncanplay",
    "oncanplaythrough", "onchange", "onclick", "onclose", "oncuechange", "ondblclick", "ondrag",
    "ondragend", "ondragenter", "ondragleave", "ondragover", "ondragstart", "ondrop",
    "ondurationchange", "onemptied", "onended", "onfocus", "oninput", "oninvalid", "onkeydown",
    "onkeypress", "onkeyup", "onloadeddata", "onloadedmetadata", "onloadstart", "onmousedown",
    "onmouseenter", "onmouseleave", "onmousemove", "onmouseout", "onmouseover", "onmouseup",
    "onmousewheel", "onpause", "onplay", "onplaying", "onprogress", "onratechange", "onreset",
    "onseeked", "onseeking", "onselect", "onshow", "onstalled", "onsubmit", "onsuspend",
    "ontimeupdate", "ontoggle", "onvolumechange", "onwaiting", "onactivate", "onfocusin",
    "onfocusout",
];

/// Port of `isExecutableUrl`.
pub fn is_executable_url(value: &str) -> bool {
    let normalized: String = value.chars().filter(|c| !matches!(c, '\t' | '\n' | '\r')).collect();
    let normalized = normalized.trim_start().to_lowercase();
    if normalized.starts_with("javascript:") || normalized.starts_with("vbscript:") {
        return true;
    }
    if !normalized.starts_with("data:") {
        return false;
    }
    let after = &normalized[5..];
    match after.find(|c| c == ';' || c == ',') {
        None => false,
        Some(end) => {
            let media = after[..end].trim();
            matches!(media, "application/xhtml+xml" | "image/svg+xml" | "text/html")
        }
    }
}

/// Port of `hasScripts` (single node only, not its subtree).
pub fn has_scripts(node: &Element) -> bool {
    if node.name == "script" && !node.children.is_empty() {
        return true;
    }
    if node.name == "a" {
        let has_exec = node.attributes.iter().any(|(k, v)| {
            (k == "href" || k.ends_with(":href")) && is_executable_url(v)
        });
        if has_exec {
            return true;
        }
    }
    EVENT_ATTRS.iter().any(|a| node.has(a))
}
