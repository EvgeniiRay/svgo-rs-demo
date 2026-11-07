//! Minimal JSON value/parser, sufficient for SVGO plugin params
//! in fixture files (objects, arrays, strings, booleans, null, numbers).

#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        match self {
            JsonValue::Object(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            JsonValue::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::String(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_array(&self) -> Option<&[JsonValue]> {
        match self {
            JsonValue::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn parse(input: &str) -> Result<JsonValue, String> {
        let chars: Vec<char> = input.chars().collect();
        let mut pos = 0usize;
        let v = parse_value(&chars, &mut pos)?;
        skip_ws(&chars, &mut pos);
        if pos != chars.len() {
            return Err(format!("json: trailing characters at {pos}"));
        }
        Ok(v)
    }
}

fn skip_ws(chars: &[char], pos: &mut usize) {
    while matches!(chars.get(*pos), Some(c) if c.is_whitespace()) {
        *pos += 1;
    }
}

fn parse_value(chars: &[char], pos: &mut usize) -> Result<JsonValue, String> {
    skip_ws(chars, pos);
    match chars.get(*pos) {
        Some('{') => {
            *pos += 1;
            let mut pairs = Vec::new();
            skip_ws(chars, pos);
            if chars.get(*pos) == Some(&'}') {
                *pos += 1;
                return Ok(JsonValue::Object(pairs));
            }
            loop {
                let key = match parse_value(chars, pos)? {
                    JsonValue::String(s) => s,
                    _ => return Err("json: object key must be a string".into()),
                };
                skip_ws(chars, pos);
                if chars.get(*pos) != Some(&':') {
                    return Err("json: expected ':'".into());
                }
                *pos += 1;
                let value = parse_value(chars, pos)?;
                pairs.push((key, value));
                skip_ws(chars, pos);
                match chars.get(*pos) {
                    Some(',') => *pos += 1,
                    Some('}') => {
                        *pos += 1;
                        return Ok(JsonValue::Object(pairs));
                    }
                    _ => return Err("json: expected ',' or '}'".into()),
                }
            }
        }
        Some('[') => {
            *pos += 1;
            let mut items = Vec::new();
            skip_ws(chars, pos);
            if chars.get(*pos) == Some(&']') {
                *pos += 1;
                return Ok(JsonValue::Array(items));
            }
            loop {
                items.push(parse_value(chars, pos)?);
                skip_ws(chars, pos);
                match chars.get(*pos) {
                    Some(',') => *pos += 1,
                    Some(']') => {
                        *pos += 1;
                        return Ok(JsonValue::Array(items));
                    }
                    _ => return Err("json: expected ',' or ']'".into()),
                }
            }
        }
        Some('"') => Ok(JsonValue::String(parse_string(chars, pos)?)),
        Some(_) => {
            let start = *pos;
            while matches!(chars.get(*pos), Some(c) if !matches!(c, ',' | '}' | ']' | ' ' | '\n' | '\r' | '\t')) {
                *pos += 1;
            }
            let lit: String = chars[start..*pos].iter().collect();
            match lit.as_str() {
                "true" => Ok(JsonValue::Bool(true)),
                "false" => Ok(JsonValue::Bool(false)),
                "null" => Ok(JsonValue::Null),
                "" => Err("json: unexpected end of input".into()),
                _ => Ok(JsonValue::Number(lit)),
            }
        }
        None => Err("json: unexpected end of input".into()),
    }
}

fn parse_string(chars: &[char], pos: &mut usize) -> Result<String, String> {
    // Assumes chars[*pos] == '"'.
    *pos += 1;
    let mut out = String::new();
    loop {
        match chars.get(*pos) {
            None => return Err("json: unterminated string".into()),
            Some('"') => {
                *pos += 1;
                return Ok(out);
            }
            Some('\\') => {
                *pos += 1;
                match chars.get(*pos) {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => out.push('\r'),
                    Some('u') => {
                        let hex: String = chars[*pos + 1..(*pos + 5).min(chars.len())].iter().collect();
                        let code = u32::from_str_radix(&hex, 16).map_err(|_| "json: bad \\u escape")?;
                        out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        *pos += 4;
                    }
                    Some(c) => out.push(*c),
                    None => return Err("json: unterminated escape".into()),
                }
                *pos += 1;
            }
            Some(c) => {
                out.push(*c);
                *pos += 1;
            }
        }
    }
}
