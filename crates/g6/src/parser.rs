//! Minimal SVG/XML parser mirroring `reference/svgo/lib/parser.js`
//! (SAX-based): text outside text elements is trimmed and empty text
//! dropped; text inside text elements is preserved verbatim.

use crate::xast::{is_text_elem, XastChild, XastElement, XastRoot};

pub fn parse_svg(data: &str) -> Result<XastRoot, String> {
    let bytes = data.as_bytes();
    let mut pos = 0usize;
    let mut root = XastRoot::default();
    // Stack of open elements; children are accumulated then attached.
    let mut stack: Vec<XastElement> = Vec::new();

    macro_rules! push_child {
        ($child:expr) => {
            if let Some(top) = stack.last_mut() {
                top.children.push($child);
            } else {
                root.children.push($child);
            }
        };
    }

    while pos < bytes.len() {
        if bytes[pos] == b'<' {
            if data[pos..].starts_with("<!--") {
                let end = data[pos + 4..].find("-->").map(|i| pos + 4 + i).ok_or("unterminated comment")?;
                push_child!(XastChild::Comment { value: data[pos + 4..end].trim().to_string() });
                pos = end + 3;
            } else if data[pos..].starts_with("<![CDATA[") {
                let end = data[pos + 9..].find("]]>").map(|i| pos + 9 + i).ok_or("unterminated cdata")?;
                push_child!(XastChild::Cdata { value: data[pos + 9..end].to_string() });
                pos = end + 3;
            } else if data[pos..].starts_with("<!DOCTYPE") || data[pos..].starts_with("<!doctype") {
                // Scan to '>' respecting an internal subset in [ ... ].
                let mut i = pos + 9;
                let mut depth = 0i32;
                while i < bytes.len() {
                    match bytes[i] {
                        b'[' => depth += 1,
                        b']' => depth -= 1,
                        b'>' if depth <= 0 => break,
                        _ => {}
                    }
                    i += 1;
                }
                if i >= bytes.len() { return Err("unterminated doctype".into()); }
                let doctype = data[pos + 9..i].to_string();
                push_child!(XastChild::Doctype { name: "svg".to_string(), doctype });
                pos = i + 1;
            } else if data[pos..].starts_with("<?") {
                let end = data[pos + 2..].find("?>").map(|i| pos + 2 + i).ok_or("unterminated instruction")?;
                let inner = &data[pos + 2..end];
                let (name, value) = match inner.find(|c: char| c.is_whitespace()) {
                    Some(idx) => (inner[..idx].to_string(), inner[idx..].trim().to_string()),
                    None => (inner.to_string(), String::new()),
                };
                push_child!(XastChild::Instruction { name, value });
                pos = end + 2;
            } else if data[pos..].starts_with("</") {
                let end = data[pos..].find('>').map(|i| pos + i).ok_or("unterminated close tag")?;
                let name = data[pos + 2..end].trim().to_string();
                let elem = stack.pop().ok_or_else(|| format!("unexpected closing tag </{name}>"))?;
                if elem.name != name {
                    return Err(format!("mismatched closing tag: expected </{}>, got </{}>", elem.name, name));
                }
                push_child!(XastChild::Element(elem));
                pos = end + 1;
            } else if data[pos..].starts_with("<!") {
                return Err(format!("unsupported markup at byte {pos}"));
            } else {
                // Open or self-closing tag.
                let (elem, self_closing, next) = parse_tag(data, pos)?;
                if self_closing {
                    push_child!(XastChild::Element(elem));
                } else {
                    stack.push(elem);
                }
                pos = next;
            }
        } else {
            let end = data[pos..].find('<').map(|i| pos + i).unwrap_or(bytes.len());
            let raw = &data[pos..end];
            if let Some(top) = stack.last_mut() {
                let decoded = decode_entities(raw);
                if is_text_elem(&top.name) {
                    if !decoded.is_empty() {
                        top.children.push(XastChild::Text { value: decoded });
                    }
                } else {
                    let trimmed = decoded.trim();
                    if !trimmed.is_empty() {
                        top.children.push(XastChild::Text { value: trimmed.to_string() });
                    }
                }
            }
            // Text at root is ignored, mirroring parser.js (current must be element).
            pos = end;
        }
    }
    if let Some(unclosed) = stack.last() {
        return Err(format!("unexpected end: unclosed <{}>", unclosed.name));
    }
    Ok(root)
}

fn parse_tag(data: &str, start: usize) -> Result<(XastElement, bool, usize), String> {
    let bytes = data.as_bytes();
    let mut i = start + 1;
    let name_start = i;
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' && bytes[i] != b'/' {
        i += 1;
    }
    let mut elem = XastElement::new(&data[name_start..i]);
    loop {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() { i += 1; }
        if i >= bytes.len() { return Err("unterminated tag".into()); }
        if bytes[i] == b'>' { return Ok((elem, false, i + 1)); }
        if bytes[i] == b'/' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'>' { return Ok((elem, true, i + 2)); }
            return Err("unexpected '/' in tag".into());
        }
        // Attribute name.
        let attr_start = i;
        while i < bytes.len() && bytes[i] != b'=' && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' && bytes[i] != b'/' {
            i += 1;
        }
        let attr_name = &data[attr_start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() { i += 1; }
        let mut value = String::new();
        if i < bytes.len() && bytes[i] == b'=' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() { i += 1; }
            if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                let quote = bytes[i];
                i += 1;
                let vstart = i;
                while i < bytes.len() && bytes[i] != quote { i += 1; }
                value = decode_entities(&data[vstart..i]);
                if i < bytes.len() { i += 1; }
            } else {
                let vstart = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' { i += 1; }
                value = decode_entities(&data[vstart..i]);
            }
        }
        if !attr_name.is_empty() {
            elem.attributes.push((attr_name.to_string(), value));
        }
    }
}

pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') { return s.to_string(); }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(idx) = rest.find('&') {
        out.push_str(&rest[..idx]);
        let tail = &rest[idx..];
        let Some(semi) = tail.find(';') else { out.push_str(tail); return out; };
        let ent = &tail[1..semi];
        let decoded: Option<String> = match ent {
            "amp" => Some("&".into()),
            "lt" => Some("<".into()),
            "gt" => Some(">".into()),
            "quot" => Some("\"".into()),
            "apos" => Some("'".into()),
            _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32).map(|c| c.to_string())
            }
            _ if ent.starts_with('#') => {
                ent[1..].parse::<u32>().ok().and_then(char::from_u32).map(|c| c.to_string())
            }
            _ => None,
        };
        match decoded {
            Some(d) => { out.push_str(&d); rest = &tail[semi + 1..]; }
            None => { out.push('&'); rest = &tail[1..]; }
        }
    }
    out.push_str(rest);
    out
}
