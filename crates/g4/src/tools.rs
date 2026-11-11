use crate::collections::{event_attr, references_prop};
use crate::xast::Element;

pub fn to_fixed(num: f64, precision: i32) -> f64 {
    let pow = 10f64.powi(precision);
    (num * pow + 0.5).floor() / pow
}
pub fn js_number(v: f64) -> String {
    if v == 0.0 { return "0".to_string(); }
    if v.fract() == 0.0 && v.abs() < 1e15 { return format!("{}", v as i64); }
    let s = format!("{}", v);
    s
}
pub fn remove_leading_zero(v: f64) -> String {
    let s = js_number(v);
    if v > 0.0 && v < 1.0 && s.starts_with('0') { return s[1..].to_string(); }
    if v > -1.0 && v < 0.0 && s.len() > 1 && s.as_bytes()[1] == b'0' { return format!("{}{}", &s[..1], &s[2..]); }
    s
}
pub fn includes_url_reference(body: &str) -> bool {
    // \burl\(("|')?#.+?\1\)
    let b = body.as_bytes();
    let mut i = 0;
    while i + 4 <= b.len() {
        if &b[i..i+4] == b"url(" {
            let mut j = i + 4;
            let mut quote: Option<u8> = None;
            if j < b.len() && (b[j]==b'"' || b[j]==b'\'') { quote = Some(b[j]); j+=1; }
            if j < b.len() && b[j]==b'#' {
                match quote {
                    None => { if body[j..].contains(')') { return true; } }
                    Some(q) => { if body[j..].contains(q as char) { return true; } }
                }
            }
            i = j;
        } else { i += 1; }
    }
    false
}
fn decode_uri(s: &str) -> String {
    // minimal percent decoding, JS decodeURI leaves reserved chars; ids in fixtures are plain
    let b = s.as_bytes(); let mut out = Vec::new(); let mut i=0;
    while i < b.len() {
        if b[i]==b'%' && i+2 < b.len() { if let Ok(v)=u8::from_str_radix(&s[i+1..i+3],16) { out.push(v); i+=3; continue; } }
        out.push(b[i]); i+=1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
pub fn find_references(attribute: &str, value: &str) -> Vec<String> {
    let mut results = Vec::new();
    if references_prop(attribute) {
        // url( quote? # capture quote? )
        let mut idx = 0;
        while let Some(pos) = value[idx..].find("url(") {
            let start = idx + pos + 4;
            let rest = &value[start..];
            let rest = rest.strip_prefix('"').or_else(|| rest.strip_prefix('\'')).unwrap_or(rest);
            if let Some(hash) = rest.strip_prefix('#') {
                let end = hash.find(['"', '\'', ')']).unwrap_or(hash.len());
                results.push(hash[..end].to_string());
            }
            idx = start;
        }
    }
    if attribute == "href" || attribute.ends_with(":href") {
        if let Some(rest) = value.strip_prefix('#') { results.push(rest.to_string()); }
    }
    if attribute == "begin" {
        // (\w+)\.[a-zA-Z]
        let chars: Vec<char> = value.chars().collect();
        for i in 0..chars.len() {
            if chars[i]=='.' && i>0 && i+1<chars.len() && chars[i+1].is_ascii_alphabetic() {
                let mut s=i;
                while s>0 && (chars[s-1].is_alphanumeric() || chars[s-1]=='_') { s-=1; }
                if s<i { results.push(chars[s..i].iter().collect()); break; }
            }
        }
    }
    results.iter().map(|r| decode_uri(r)).collect()
}
pub fn is_executable_url(value: &str) -> bool {
    let normalized: String = value.chars().filter(|c| !matches!(c, '\t'|'\n'|'\r')).collect::<String>().trim_start().to_lowercase();
    if normalized.starts_with("javascript:") || normalized.starts_with("vbscript:") { return true; }
    if !normalized.starts_with("data:") { return false; }
    let rest = &normalized[5..];
    let Some(end) = rest.find([';', ',']) else { return false };
    let mt = rest[..end].trim();
    matches!(mt, "application/xhtml+xml"|"image/svg+xml"|"text/html")
}
pub fn has_scripts(node: &Element) -> bool {
    if node.name=="script" && !node.children.is_empty() { return true; }
    if node.name=="a" {
        for (k,v) in &node.attributes { if (k=="href" || k.ends_with(":href")) && is_executable_url(v) { return true; } }
    }
    node.attributes.iter().any(|(k,_)| event_attr(k))
}
