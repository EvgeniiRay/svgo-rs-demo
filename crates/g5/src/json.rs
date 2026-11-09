#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        if let Json::Obj(m) = self {
            m.iter().find(|(k, _)| k == key).map(|(_, v)| v)
        } else {
            None
        }
    }
    pub fn as_bool(&self, default: bool) -> bool {
        match self.get_bool_direct() {
            Some(b) => b,
            None => default,
        }
    }
    fn get_bool_direct(&self) -> Option<bool> {
        if let Json::Bool(b) = self { Some(*b) } else { None }
    }
    pub fn bool_param(&self, key: &str, default: bool) -> bool {
        match self.get(key) {
            Some(Json::Bool(b)) => *b,
            _ => default,
        }
    }
    pub fn num_param(&self, key: &str, default: f64) -> f64 {
        match self.get(key) {
            Some(Json::Num(n)) => *n,
            _ => default,
        }
    }
    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
}

pub fn parse_json(s: &str) -> Json {
    let bytes: Vec<char> = s.chars().collect();
    let mut pos = 0usize;
    let v = parse_value(&bytes, &mut pos);
    v.unwrap_or(Json::Null)
}

fn skip_ws(b: &[char], p: &mut usize) {
    while *p < b.len() && b[*p].is_whitespace() { *p += 1; }
}

fn parse_value(b: &[char], p: &mut usize) -> Option<Json> {
    skip_ws(b, p);
    if *p >= b.len() { return None; }
    match b[*p] {
        '{' => {
            *p += 1;
            let mut obj = Vec::new();
            loop {
                skip_ws(b, p);
                if *p < b.len() && b[*p] == '}' { *p += 1; break; }
                let key = match parse_value(b, p) {
                    Some(Json::Str(s)) => s,
                    _ => break,
                };
                skip_ws(b, p);
                if *p < b.len() && b[*p] == ':' { *p += 1; }
                let val = parse_value(b, p)?;
                obj.push((key, val));
                skip_ws(b, p);
                if *p < b.len() && b[*p] == ',' { *p += 1; }
            }
            Some(Json::Obj(obj))
        }
        '[' => {
            *p += 1;
            let mut arr = Vec::new();
            loop {
                skip_ws(b, p);
                if *p < b.len() && b[*p] == ']' { *p += 1; break; }
                arr.push(parse_value(b, p)?);
                skip_ws(b, p);
                if *p < b.len() && b[*p] == ',' { *p += 1; }
            }
            Some(Json::Arr(arr))
        }
        '"' => {
            *p += 1;
            let mut out = String::new();
            while *p < b.len() && b[*p] != '"' {
                if b[*p] == '\\' && *p + 1 < b.len() {
                    *p += 1;
                    match b[*p] {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        'r' => out.push('\r'),
                        'u' => {
                            let hex: String = b[*p + 1..(*p + 5).min(b.len())].iter().collect();
                            if let Ok(n) = u32::from_str_radix(&hex, 16) {
                                if let Some(c) = char::from_u32(n) { out.push(c); }
                            }
                            *p += 4;
                        }
                        c => out.push(c),
                    }
                } else {
                    out.push(b[*p]);
                }
                *p += 1;
            }
            *p += 1;
            Some(Json::Str(out))
        }
        't' => { *p += 4; Some(Json::Bool(true)) }
        'f' => { *p += 5; Some(Json::Bool(false)) }
        'n' => { *p += 4; Some(Json::Null) }
        _ => {
            let start = *p;
            while *p < b.len() && (b[*p].is_ascii_alphanumeric() || "+-.eE".contains(b[*p])) { *p += 1; }
            let txt: String = b[start..*p].iter().collect();
            txt.parse::<f64>().ok().map(Json::Num)
        }
    }
}
