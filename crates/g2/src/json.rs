//! Minimal JSON parser for plugin params (no external crates).

#[derive(Debug, Clone, PartialEq)]
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
        match self {
            Json::Obj(pairs) => pairs.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    /// JS truthiness for booleans/strings/numbers (absent => None).
    pub fn truthy(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            Json::Str(s) => Some(!s.is_empty()),
            Json::Num(n) => Some(*n != 0.0),
            Json::Null => Some(false),
            Json::Arr(_) | Json::Obj(_) => Some(true),
        }
    }
    pub fn as_bool(&self, default: bool) -> bool {
        self.truthy().unwrap_or(default)
    }
    pub fn as_str_list(&self) -> Vec<String> {
        match self {
            Json::Arr(items) => items
                .iter()
                .filter_map(|i| match i {
                    Json::Str(s) => Some(s.clone()),
                    _ => None,
                })
                .collect(),
            Json::Str(s) => vec![s.clone()],
            _ => Vec::new(),
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
}

pub fn parse(input: &str) -> Result<Json, String> {
    let chars: Vec<char> = input.chars().collect();
    let mut p = Parser { chars: &chars, pos: 0 };
    p.skip_ws();
    if p.pos >= chars.len() {
        return Ok(Json::Null);
    }
    let v = p.value()?;
    Ok(v)
}

struct Parser<'a> {
    chars: &'a [char],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn skip_ws(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    fn value(&mut self) -> Result<Json, String> {
        self.skip_ws();
        match self.peek().ok_or("eof")? {
            '{' => {
                self.pos += 1;
                let mut pairs = Vec::new();
                loop {
                    self.skip_ws();
                    if self.peek() == Some('}') {
                        self.pos += 1;
                        break;
                    }
                    let key = match self.value()? {
                        Json::Str(s) => s,
                        _ => return Err("obj key".into()),
                    };
                    self.skip_ws();
                    if self.peek() == Some(':') {
                        self.pos += 1;
                    }
                    let val = self.value()?;
                    pairs.push((key, val));
                    self.skip_ws();
                    if self.peek() == Some(',') {
                        self.pos += 1;
                    }
                }
                Ok(Json::Obj(pairs))
            }
            '[' => {
                self.pos += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_ws();
                    if self.peek() == Some(']') {
                        self.pos += 1;
                        break;
                    }
                    items.push(self.value()?);
                    self.skip_ws();
                    if self.peek() == Some(',') {
                        self.pos += 1;
                    }
                }
                Ok(Json::Arr(items))
            }
            '"' => Ok(Json::Str(self.string()?)),
            't' => {
                self.lit("true")?;
                Ok(Json::Bool(true))
            }
            'f' => {
                self.lit("false")?;
                Ok(Json::Bool(false))
            }
            'n' => {
                self.lit("null")?;
                Ok(Json::Null)
            }
            _ => {
                let start = self.pos;
                while self.pos < self.chars.len()
                    && matches!(self.chars[self.pos], '0'..='9' | '-' | '+' | '.' | 'e' | 'E')
                {
                    self.pos += 1;
                }
                let s: String = self.chars[start..self.pos].iter().collect();
                s.parse::<f64>().map(Json::Num).map_err(|e| e.to_string())
            }
        }
    }
    fn lit(&mut self, s: &str) -> Result<(), String> {
        for c in s.chars() {
            if self.peek() != Some(c) {
                return Err(format!("expected {s}"));
            }
            self.pos += 1;
        }
        Ok(())
    }
    fn string(&mut self) -> Result<String, String> {
        // assumes peek == '"'
        self.pos += 1;
        let mut out = String::new();
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                '"' => return Ok(out),
                '\\' => {
                    let e = self.peek().ok_or("eof")?;
                    self.pos += 1;
                    match e {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        'r' => out.push('\r'),
                        'u' => {
                            let hex: String = self.chars[self.pos..self.pos + 4].iter().collect();
                            self.pos += 4;
                            let code = u32::from_str_radix(&hex, 16).map_err(|e| e.to_string())?;
                            out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        }
                        other => out.push(other),
                    }
                }
                other => out.push(other),
            }
        }
        Err("unterminated string".into())
    }
}
