//! Ports of helpers from `lib/svgo/tools.js`.

use crate::collections::is_references_prop;
use crate::dom::Element;

fn is_executable_url(value: &str) -> bool {
    let normalized: String = value
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect::<String>()
        .trim_start()
        .to_lowercase();
    if normalized.starts_with("javascript:") || normalized.starts_with("vbscript:") {
        return true;
    }
    if !normalized.starts_with("data:") {
        return false;
    }
    let rest = &normalized[5..];
    match rest.find([';', ',']) {
        None => false,
        Some(end) => matches!(
            rest[..end].trim(),
            "application/xhtml+xml" | "image/svg+xml" | "text/html"
        ),
    }
}

/// Port of `hasScripts` (event attribute check approximated by `on*` prefix,
/// which covers the JS event-attribute sets used there).
pub fn has_scripts(node: &Element) -> bool {
    if node.name == "script" && !node.children.is_empty() {
        return true;
    }
    if node.name == "a" {
        for (k, v) in &node.attrs {
            if (k == "href" || k.ends_with(":href")) && is_executable_url(v) {
                return true;
            }
        }
    }
    node.attrs.iter().any(|(k, _)| k.starts_with("on"))
}

fn percent_decode_uri(s: &str) -> String {
    // decodeURI: does not decode escapes of reserved chars ; / ? : @ & = + $ - _ . ! ~ * ' ( ) #
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &s[i + 1..i + 3];
            if let Ok(b) = u8::from_str_radix(hex, 16) {
                let c = b as char;
                if matches!(c, ';' | '/' | '?' | ':' | '@' | '&' | '=' | '+' | '$' | ',' | '#') {
                    out.extend_from_slice(&bytes[i..i + 3]);
                } else {
                    out.push(b);
                }
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Port of `encodeURI` (leaves URI-reserved and mark chars unescaped).
pub fn encode_uri(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric()
            || matches!(c, ';' | '/' | '?' | ':' | '@' | '&' | '=' | '+' | '$' | '-' | '_' | '.' | '!' | '~' | '*' | '\'' | '(' | ')' | '#' | ',')
        {
            out.push(c);
        } else {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).as_bytes() {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

/// Port of `findReferences`.
pub fn find_references(attribute: &str, value: &str) -> Vec<String> {
    let mut results: Vec<String> = Vec::new();
    if is_references_prop(attribute) {
        // \burl\(["']?#(.+?)\1\)
        let chars: Vec<char> = value.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if (i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_' || chars[i - 1] == '-'))
                && chars[i..].iter().collect::<String>().starts_with("url(")
            {
                let mut j = i + 4;
                let quote = if j < chars.len() && (chars[j] == '"' || chars[j] == '\'') {
                    let q = chars[j];
                    j += 1;
                    Some(q)
                } else {
                    None
                };
                if j < chars.len() && chars[j] == '#' {
                    j += 1;
                    let start = j;
                    let mut found = None;
                    while j < chars.len() {
                        match quote {
                            Some(q) => {
                                if chars[j] == q {
                                    found = Some(chars[start..j].iter().collect::<String>());
                                    break;
                                }
                            }
                            None => {
                                if chars[j] == ')' {
                                    found = Some(chars[start..j].iter().collect::<String>());
                                    break;
                                }
                            }
                        }
                        j += 1;
                    }
                    if let Some(f) = found {
                        if !f.is_empty() {
                            results.push(f);
                        }
                    }
                }
                i += 4;
            } else {
                i += 1;
            }
        }
    }
    if attribute == "href" || attribute.ends_with(":href") {
        if let Some(rest) = value.strip_prefix('#') {
            if !rest.is_empty() {
                results.push(rest.to_string());
            }
        }
    }
    if attribute == "begin" {
        // /(\w+)\.[a-zA-Z]/
        let chars: Vec<char> = value.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i].is_ascii_alphanumeric() || chars[i] == '_' {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                if i < chars.len()
                    && chars[i] == '.'
                    && i + 1 < chars.len()
                    && chars[i + 1].is_ascii_alphabetic()
                {
                    results.push(chars[start..i].iter().collect());
                    break;
                }
            } else {
                i += 1;
            }
        }
    }
    results.iter().map(|r| percent_decode_uri(r)).collect()
}
