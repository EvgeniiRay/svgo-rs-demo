//! Minimal XML parser mirroring lib/parser.js (sax) semantics needed by the
//! ports: comments are trimmed, text outside text elements is trimmed and
//! dropped when empty, text inside text elements is kept verbatim, entity
//! references (predefined, numeric, and DOCTYPE-declared) are decoded.

use crate::collections::TEXT_ELEMS;
use crate::xast::{Child, Element, Root};
use std::collections::HashMap;

pub fn parse_svg(data: &str) -> Result<Root, String> {
    let mut p = Parser { s: data.as_bytes(), pos: 0, entities: HashMap::new() };
    let mut root = Root::default();
    p.parse_children(&mut root.children, None)?;
    Ok(root)
}

struct Parser<'a> {
    s: &'a [u8],
    pos: usize,
    entities: HashMap<String, String>,
}

impl<'a> Parser<'a> {
    fn rest(&self) -> &'a str {
        std::str::from_utf8(&self.s[self.pos..]).unwrap_or("")
    }
    fn starts(&self, pat: &str) -> bool {
        self.rest().starts_with(pat)
    }
    fn bump(&mut self, n: usize) {
        self.pos += n;
    }
    fn parse_children(&mut self, out: &mut Vec<Child>, parent: Option<&str>) -> Result<(), String> {
        loop {
            if self.pos >= self.s.len() {
                return Ok(());
            }
            if self.starts("</") {
                return Ok(());
            } else if self.starts("<!--") {
                self.bump(4);
                let end = self.rest().find("-->").ok_or("unterminated comment")?;
                let v = self.rest()[..end].trim().to_string();
                self.bump(end + 3);
                out.push(Child::Comment(v));
            } else if self.starts("<![CDATA[") {
                self.bump(9);
                let end = self.rest().find("]]>").ok_or("unterminated cdata")?;
                let v = self.rest()[..end].to_string();
                self.bump(end + 3);
                out.push(Child::Cdata(v));
            } else if self.starts("<!DOCTYPE") {
                let start = self.pos;
                // scan to '>' respecting an internal subset in [ ... ]
                let mut depth = 0i32;
                let mut i = self.pos;
                while i < self.s.len() {
                    match self.s[i] {
                        b'[' => depth += 1,
                        b']' => depth -= 1,
                        b'>' if depth <= 0 => break,
                        _ => {}
                    }
                    i += 1;
                }
                let full = std::str::from_utf8(&self.s[start..=i.min(self.s.len() - 1)]).unwrap_or("").to_string();
                let doctype = format!(" {}", full.trim_start_matches("<!DOCTYPE").trim_end_matches('>').trim());
                self.collect_entities(&full);
                self.pos = i + 1;
                out.push(Child::Doctype { name: "svg".into(), data: doctype });
            } else if self.starts("<?") {
                self.bump(2);
                let end = self.rest().find("?>").ok_or("unterminated instruction")?;
                let body = self.rest()[..end].to_string();
                self.bump(end + 2);
                let mut parts = body.splitn(2, |c: char| c.is_whitespace());
                let name = parts.next().unwrap_or("").to_string();
                let value = parts.next().unwrap_or("").trim_start().to_string();
                out.push(Child::Instruction { name, value });
            } else if self.starts("<") {
                let el = self.parse_element()?;
                out.push(Child::Element(el));
            } else {
                let end = self.rest().find('<').map(|i| i).unwrap_or(self.rest().len());
                let raw = self.decode_entities(&self.rest()[..end].to_string());
                self.bump(end);
                let in_text = parent.map(|n| TEXT_ELEMS.contains(&n)).unwrap_or(false);
                if in_text {
                    if !raw.is_empty() {
                        out.push(Child::Text(raw));
                    }
                } else {
                    let v = raw.trim().to_string();
                    if !v.is_empty() {
                        out.push(Child::Text(v));
                    }
                }
            }
        }
    }

    fn collect_entities(&mut self, doctype: &str) {
        // <!ENTITY name "value"> or 'value'
        let mut rest = doctype;
        while let Some(i) = rest.find("<!ENTITY") {
            rest = &rest[i + 8..];
            let mut it = rest.splitn(2, |c: char| c.is_whitespace());
            let _ = it.next();
            let after = it.next().unwrap_or("");
            let mut parts = after.splitn(2, |c: char| c.is_whitespace());
            let name = parts.next().unwrap_or("").to_string();
            let valpart = parts.next().unwrap_or("").trim_start();
            if let Some(q) = valpart.chars().next() {
                if q == '"' || q == '\'' {
                    if let Some(end) = valpart[1..].find(q) {
                        self.entities.insert(name, valpart[1..1 + end].to_string());
                    }
                }
            }
        }
    }

    fn decode_entities(&self, s: &str) -> String {
        if !s.contains('&') {
            return s.to_string();
        }
        let mut out = String::with_capacity(s.len());
        let mut rest = s;
        while let Some(i) = rest.find('&') {
            out.push_str(&rest[..i]);
            let tail = &rest[i + 1..];
            match tail.find(';') {
                Some(j) if j < 32 => {
                    let ent = &tail[..j];
                    let decoded: Option<String> = match ent {
                        "amp" => Some("&".into()),
                        "lt" => Some("<".into()),
                        "gt" => Some(">".into()),
                        "quot" => Some("\"".into()),
                        "apos" => Some("'".into()),
                        _ => {
                            if let Some(hex) = ent.strip_prefix("#x").or_else(|| ent.strip_prefix("#X")) {
                                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32).map(|c| c.to_string())
                            } else if let Some(dec) = ent.strip_prefix('#') {
                                dec.parse::<u32>().ok().and_then(char::from_u32).map(|c| c.to_string())
                            } else {
                                self.entities.get(ent).cloned()
                            }
                        }
                    };
                    match decoded {
                        Some(d) => out.push_str(&d),
                        None => {
                            out.push('&');
                            out.push_str(ent);
                            out.push(';');
                        }
                    }
                    rest = &tail[j + 1..];
                }
                _ => {
                    out.push('&');
                    rest = tail;
                }
            }
        }
        out.push_str(rest);
        out
    }

    fn parse_element(&mut self) -> Result<Element, String> {
        // at '<'
        self.bump(1);
        let name = self.read_name();
        let mut el = Element::new(&name);
        loop {
            self.skip_ws();
            if self.starts("/>") {
                self.bump(2);
                return Ok(el);
            }
            if self.starts(">") {
                self.bump(1);
                break;
            }
            if self.pos >= self.s.len() {
                return Err("unterminated tag".into());
            }
            let aname = self.read_attr_name();
            self.skip_ws();
            let mut value = String::new();
            if self.starts("=") {
                self.bump(1);
                self.skip_ws();
                if self.pos < self.s.len() {
                    let q = self.s[self.pos] as char;
                    if q == '"' || q == '\'' {
                        self.bump(1);
                        let end = self.rest().find(q).ok_or("unterminated attribute")?;
                        value = self.decode_entities(&self.rest()[..end].to_string());
                        self.bump(end + 1);
                    }
                }
            }
            if !aname.is_empty() {
                el.set(&aname, &value);
            }
        }
        let parent_name = el.name.clone();
        self.parse_children(&mut el.children, Some(&parent_name))?;
        // closing tag
        if self.starts("</") {
            self.bump(2);
            let _close = self.read_name();
            self.skip_ws();
            if self.starts(">") {
                self.bump(1);
            }
        }
        Ok(el)
    }

    fn skip_ws(&mut self) {
        while self.pos < self.s.len() && (self.s[self.pos] as char).is_whitespace() {
            self.pos += 1;
        }
    }
    fn read_name(&mut self) -> String {
        let start = self.pos;
        while self.pos < self.s.len() {
            let c = self.s[self.pos] as char;
            if c.is_whitespace() || c == '/' || c == '>' || c == '=' {
                break;
            }
            self.pos += 1;
        }
        std::str::from_utf8(&self.s[start..self.pos]).unwrap_or("").to_string()
    }
    fn read_attr_name(&mut self) -> String {
        self.read_name()
    }
}
