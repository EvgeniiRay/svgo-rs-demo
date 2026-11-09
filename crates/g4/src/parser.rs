use crate::xast::{is_text_elem, Element, Node, Root};

pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') { return s.to_string(); }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i..];
        if let Some(end) = after.find(';') {
            let ent = &after[1..end];
            let decoded: Option<String> = match ent {
                "amp" => Some("&".into()), "lt" => Some("<".into()), "gt" => Some(">".into()),
                "quot" => Some("\"".into()), "apos" => Some("'".into()),
                _ if ent.starts_with("#x") || ent.starts_with("#X") => u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32).map(|c| c.to_string()),
                _ if ent.starts_with('#') => ent[1..].parse::<u32>().ok().and_then(char::from_u32).map(|c| c.to_string()),
                _ => None,
            };
            if let Some(d) = decoded { out.push_str(&d); rest = &after[end+1..]; continue; }
        }
        out.push('&'); rest = &after[1..];
    }
    out.push_str(rest); out
}

struct P<'a> { b: &'a [u8], pos: usize }
impl<'a> P<'a> {
    fn starts(&self, s: &str) -> bool { self.b[self.pos..].starts_with(s.as_bytes()) }
    fn text_until(&mut self, end: usize) -> String { let t = String::from_utf8_lossy(&self.b[self.pos..end]).into_owned(); self.pos = end; t }
}
pub fn parse_svg(data: &str) -> Root {
    let mut p = P { b: data.as_bytes(), pos: 0 };
    let children = parse_children(&mut p, None, false);
    Root { children }
}
fn find_from(b: &[u8], from: usize, needle: &str) -> usize {
    let n = needle.as_bytes();
    if n.is_empty() || from >= b.len() { return b.len(); }
    (from..=b.len()-n.len()).find(|&i| &b[i..i+n.len()]==n).unwrap_or(b.len())
}
fn parse_children(p: &mut P, closing: Option<&str>, parent_text: bool) -> Vec<Node> {
    let mut out: Vec<Node> = Vec::new();
    loop {
        if p.pos >= p.b.len() { break; }
        if p.starts("</") {
            let end = find_from(p.b, p.pos, ">");
            let name = String::from_utf8_lossy(&p.b[p.pos+2..end]).trim().to_string();
            p.pos = if end < p.b.len() { end+1 } else { end };
            if closing.map(|c| c==name).unwrap_or(false) { break; } else if closing.is_some() { break; } else { continue; }
        }
        if p.starts("<!--") {
            let end = find_from(p.b, p.pos+4, "-->");
            let raw = p.text_until(end); p.pos = if end+3<=p.b.len() { end+3 } else { p.b.len() };
            out.push(Node::Comment(raw["<!--".len()..].trim().to_string())); continue;
        }
        if p.starts("<![CDATA[") {
            p.pos += 9;
            let end = find_from(p.b, p.pos, "]]>");
            let raw = p.text_until(end); p.pos = if end+3<=p.b.len() { end+3 } else { p.b.len() };
            out.push(Node::CData(raw)); continue;
        }
        if p.starts("<!DOCTYPE") || p.starts("<!doctype") {
            let mut i = p.pos; let mut depth = 0i32; let mut q: Option<u8> = None;
            while i < p.b.len() { let c=p.b[i]; if let Some(qc)=q { if c==qc { q=None; } } else if c==b'"'||c==b'\'' { q=Some(c); } else if c==b'[' { depth+=1; } else if c==b']' { depth-=1; } else if c==b'>' && depth<=0 { break; } i+=1; }
            let raw = String::from_utf8_lossy(&p.b[p.pos..i]).into_owned();
            p.pos = if i < p.b.len() { i+1 } else { i };
            let inner = raw.strip_prefix("<!DOCTYPE").or_else(|| raw.strip_prefix("<!doctype")).unwrap_or("").to_string();
            out.push(Node::Doctype(inner)); continue;
        }
        if p.starts("<?") {
            let end = find_from(p.b, p.pos+2, "?>");
            let raw = p.text_until(end); p.pos = if end+2<=p.b.len() { end+2 } else { p.b.len() };
            let inner = raw[2..].trim_start().to_string();
            let (name, value) = match inner.split_once(|c: char| c.is_whitespace()) { Some((a,b)) => (a.to_string(), b.trim().to_string()), None => (inner, String::new()) };
            out.push(Node::Instruction { name, value }); continue;
        }
        if p.starts("<!") {
            let end = find_from(p.b, p.pos, ">");
            p.pos = if end < p.b.len() { end+1 } else { end }; continue;
        }
        if p.starts("<") {
            // open tag
            let mut i = p.pos+1;
            while i < p.b.len() && !matches!(p.b[i], b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/') { i+=1; }
            let name = String::from_utf8_lossy(&p.b[p.pos+1..i]).into_owned();
            if name.is_empty() { p.pos += 1; continue; }
            p.pos = i;
            let mut el = Element::new(&name);
            let self_closing;
            loop {
                while p.pos < p.b.len() && matches!(p.b[p.pos], b' ' | b'\t' | b'\r' | b'\n') { p.pos+=1; }
                if p.starts("/>") { p.pos+=2; self_closing=true; break; }
                if p.starts(">") { p.pos+=1; self_closing=false; break; }
                if p.pos>=p.b.len() { self_closing=true; break; }
                let s=p.pos;
                while p.pos < p.b.len() && !matches!(p.b[p.pos], b'=' | b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/') { p.pos+=1; }
                let aname = String::from_utf8_lossy(&p.b[s..p.pos]).into_owned();
                while p.pos < p.b.len() && matches!(p.b[p.pos], b' ' | b'\t' | b'\r' | b'\n') { p.pos+=1; }
                let mut aval = String::new();
                if p.starts("=") {
                    p.pos+=1;
                    while p.pos < p.b.len() && matches!(p.b[p.pos], b' ' | b'\t' | b'\r' | b'\n') { p.pos+=1; }
                    if p.pos < p.b.len() && (p.b[p.pos]==b'"' || p.b[p.pos]==b'\'') {
                        let qc=p.b[p.pos]; p.pos+=1; let s=p.pos;
                        while p.pos < p.b.len() && p.b[p.pos]!=qc { p.pos+=1; }
                        aval = decode_entities(&String::from_utf8_lossy(&p.b[s..p.pos]).into_owned());
                        if p.pos < p.b.len() { p.pos+=1; }
                    } else {
                        let s=p.pos;
                        while p.pos < p.b.len() && !matches!(p.b[p.pos], b' ' | b'\t' | b'\r' | b'\n' | b'>') { p.pos+=1; }
                        aval = decode_entities(&String::from_utf8_lossy(&p.b[s..p.pos]).into_owned());
                    }
                }
                if !aname.is_empty() { el.set(&aname, &aval); }
            }
            if !self_closing {
                let is_text = is_text_elem(&el.name);
                el.children = parse_children(p, Some(&name.clone()), is_text);
            }
            out.push(Node::Element(el)); continue;
        }
        // text
        let end = find_from(p.b, p.pos, "<");
        let raw = p.text_until(end);
        let decoded = decode_entities(&raw);
        if parent_text {
            out.push(Node::Text(decoded));
        } else {
            let t = decoded.trim().to_string();
            if !t.is_empty() { out.push(Node::Text(t)); }
        }
    }
    out
}
