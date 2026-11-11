//! Small CSS parser / generator / minifier standing in for css-tree + CSSO.
//! Covers the stylesheet shapes SVGO's style plugins encounter in fixtures:
//! rules, selector lists, declaration blocks and block/statement at-rules.

#[derive(Debug, Clone, PartialEq)]
pub struct Decl {
    pub property: String,
    pub value: String,
    pub important: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AtruleBlock {
    Rules(Vec<CssNode>),
    Decls(Vec<Decl>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum CssNode {
    Rule { selectors: Vec<String>, decls: Vec<Decl> },
    Atrule { name: String, prelude: String, block: Option<AtruleBlock> },
}

// ---------- low-level scanning ----------
pub fn strip_comments(css: &str) -> String {
    let chars: Vec<char> = css.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let mut quote: Option<char> = None;
    while i < chars.len() {
        let c = chars[i];
        match quote {
            Some(q) => {
                out.push(c);
                if c == '\\' && i + 1 < chars.len() {
                    out.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if c == q {
                    quote = None;
                }
                i += 1;
            }
            None => {
                if c == '"' || c == '\'' {
                    quote = Some(c);
                    out.push(c);
                    i += 1;
                } else if c == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
                    i += 2;
                    while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                        i += 1;
                    }
                    i = (i + 2).min(chars.len());
                    out.push(' ');
                } else {
                    out.push(c);
                    i += 1;
                }
            }
        }
    }
    out
}

/// Split on a separator at paren/bracket depth 0, respecting strings.
pub fn split_top_level(s: &str, sep: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth_paren = 0i32;
    let mut depth_brack = 0i32;
    let mut quote: Option<char> = None;
    let mut cur = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                cur.push(c);
                if c == '\\' {
                    if let Some(n) = chars.next() {
                        cur.push(n);
                    }
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => {
                    quote = Some(c);
                    cur.push(c);
                }
                '(' => {
                    depth_paren += 1;
                    cur.push(c);
                }
                ')' => {
                    depth_paren -= 1;
                    cur.push(c);
                }
                '[' => {
                    depth_brack += 1;
                    cur.push(c);
                }
                ']' => {
                    depth_brack -= 1;
                    cur.push(c);
                }
                _ if c == sep && depth_paren == 0 && depth_brack == 0 => {
                    parts.push(cur.clone());
                    cur.clear();
                }
                _ => cur.push(c),
            },
        }
    }
    parts.push(cur);
    parts
}

pub fn parse_decl_list(input: &str) -> Vec<Decl> {
    let cleaned = strip_comments(input);
    let mut decls = Vec::new();
    for part in split_top_level(&cleaned, ';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        // first ':' at depth 0
        let mut depth = 0i32;
        let mut quote: Option<char> = None;
        let mut colon = None;
        for (i, c) in part.char_indices() {
            match quote {
                Some(q) => {
                    if c == q {
                        quote = None;
                    }
                }
                None => match c {
                    '"' | '\'' => quote = Some(c),
                    '(' | '[' => depth += 1,
                    ')' | ']' => depth -= 1,
                    ':' if depth == 0 => {
                        colon = Some(i);
                        break;
                    }
                    _ => {}
                },
            }
        }
        let Some(ci) = colon else { continue };
        let property = part[..ci].trim().to_string();
        let mut value = part[ci + 1..].trim().to_string();
        let mut important = false;
        let lower = value.to_lowercase();
        if let Some(pos) = lower.rfind("!important") {
            if lower[pos + 10..].trim().is_empty() {
                important = true;
                value = value[..pos].trim().to_string();
            }
        }
        if !property.is_empty() {
            decls.push(Decl { property, value, important });
        }
    }
    decls
}

// ---------- stylesheet parser ----------
struct CssParser {
    chars: Vec<char>,
    pos: usize,
}
impl CssParser {
    fn skip_ws(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    /// Read until one of `stops` at depth 0 (not consumed).
    fn read_until(&mut self, stops: &[char]) -> String {
        let mut out = String::new();
        let mut paren = 0i32;
        let mut brack = 0i32;
        let mut quote: Option<char> = None;
        while self.pos < self.chars.len() {
            let c = self.chars[self.pos];
            match quote {
                Some(q) => {
                    out.push(c);
                    self.pos += 1;
                    if c == '\\' && self.pos < self.chars.len() {
                        out.push(self.chars[self.pos]);
                        self.pos += 1;
                    } else if c == q {
                        quote = None;
                    }
                }
                None => {
                    if paren == 0 && brack == 0 && stops.contains(&c) {
                        break;
                    }
                    match c {
                        '"' | '\'' => {
                            quote = Some(c);
                            out.push(c);
                        }
                        '(' => {
                            paren += 1;
                            out.push(c);
                        }
                        ')' => {
                            paren -= 1;
                            out.push(c);
                        }
                        '[' => {
                            brack += 1;
                            out.push(c);
                        }
                        ']' => {
                            brack -= 1;
                            out.push(c);
                        }
                        _ => out.push(c),
                    }
                    self.pos += 1;
                }
            }
        }
        out
    }
    fn parse_nodes(&mut self) -> Vec<CssNode> {
        let mut nodes = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                None | Some('}') => break,
                Some('@') => {
                    self.pos += 1;
                    let mut name = String::new();
                    while let Some(c) = self.peek() {
                        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                            name.push(c);
                            self.pos += 1;
                        } else {
                            break;
                        }
                    }
                    let prelude = self.read_until(&['{', ';']).trim().to_string();
                    match self.peek() {
                        Some(';') => {
                            self.pos += 1;
                            nodes.push(CssNode::Atrule { name, prelude, block: None });
                        }
                        Some('{') => {
                            self.pos += 1;
                            let inner = self.read_block_raw();
                            let block = if contains_top_brace(&inner) {
                                AtruleBlock::Rules(parse_stylesheet(&inner))
                            } else {
                                AtruleBlock::Decls(parse_decl_list(&inner))
                            };
                            nodes.push(CssNode::Atrule { name, prelude, block: Some(block) });
                        }
                        _ => nodes.push(CssNode::Atrule { name, prelude, block: None }),
                    }
                }
                Some(_) => {
                    let selector_text = self.read_until(&['{']);
                    if self.peek() == Some('{') {
                        self.pos += 1;
                    } else {
                        break;
                    }
                    let inner = self.read_block_raw();
                    let decls = parse_decl_list(&inner);
                    let selectors = split_top_level(selector_text.trim(), ',')
                        .iter()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    nodes.push(CssNode::Rule { selectors, decls });
                }
            }
        }
        nodes
    }
    /// Assumes opening '{' consumed; reads through matching '}' (consumed).
    fn read_block_raw(&mut self) -> String {
        let mut out = String::new();
        let mut depth = 1i32;
        let mut quote: Option<char> = None;
        while self.pos < self.chars.len() {
            let c = self.chars[self.pos];
            self.pos += 1;
            match quote {
                Some(q) => {
                    out.push(c);
                    if c == '\\' && self.pos < self.chars.len() {
                        out.push(self.chars[self.pos]);
                        self.pos += 1;
                    } else if c == q {
                        quote = None;
                    }
                }
                None => match c {
                    '"' | '\'' => {
                        quote = Some(c);
                        out.push(c);
                    }
                    '{' => {
                        depth += 1;
                        out.push(c);
                    }
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        out.push(c);
                    }
                    _ => out.push(c),
                },
            }
        }
        out
    }
}

fn contains_top_brace(s: &str) -> bool {
    let mut quote: Option<char> = None;
    for c in s.chars() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => quote = Some(c),
                '{' => return true,
                _ => {}
            },
        }
    }
    false
}

pub fn parse_stylesheet(css: &str) -> Vec<CssNode> {
    let cleaned = strip_comments(css);
    let mut p = CssParser { chars: cleaned.chars().collect(), pos: 0 };
    p.parse_nodes()
}

// ---------- generation ----------
/// Collapse whitespace outside strings to single spaces.
fn collapse_ws(s: &str) -> String {
    let mut out = String::new();
    let mut quote: Option<char> = None;
    let mut pending_space = false;
    for c in s.chars() {
        match quote {
            Some(q) => {
                out.push(c);
                if c == q {
                    quote = None;
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    if pending_space && !out.is_empty() {
                        out.push(' ');
                    }
                    pending_space = false;
                    quote = Some(c);
                    out.push(c);
                } else if c.is_whitespace() {
                    pending_space = true;
                } else {
                    if pending_space && !out.is_empty() {
                        out.push(' ');
                    }
                    pending_space = false;
                    out.push(c);
                }
            }
        }
    }
    out
}

/// Unquote `url('x')` when safe and normalize string quotes to double quotes
/// (css-tree generation behaviour).
fn normalize_url_and_strings(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if (chars[i] == 'u' || chars[i] == 'U')
            && chars[i..].iter().collect::<String>().to_lowercase().starts_with("url(")
        {
            // find matching ')'
            let mut j = i + 4;
            let mut inner = String::new();
            let mut quote: Option<char> = None;
            let mut quote_used: Option<char> = None;
            while j < chars.len() {
                let c = chars[j];
                match quote {
                    Some(q) => {
                        if c == q {
                            quote = None;
                        } else {
                            inner.push(c);
                        }
                    }
                    None => {
                        if c == '"' || c == '\'' {
                            quote = Some(c);
                            quote_used = Some(c);
                        } else if c == ')' {
                            break;
                        } else {
                            inner.push(c);
                        }
                    }
                }
                j += 1;
            }
            let inner = inner.trim().to_string();
            if inner.chars().all(|c| !c.is_whitespace() && c != '"' && c != '\'' && c != '(' && c != ')') {
                out.push_str(&format!("url({inner})"));
            } else if let Some(q) = quote_used {
                out.push_str(&format!("url({q}{inner}{q})"));
            } else {
                out.push_str(&format!("url({inner})"));
            }
            i = j + 1;
        } else if chars[i] == '\'' {
            // single-quoted string -> double-quoted
            let mut j = i + 1;
            let mut inner = String::new();
            while j < chars.len() && chars[j] != '\'' {
                inner.push(chars[j]);
                j += 1;
            }
            out.push('"');
            out.push_str(&inner);
            out.push('"');
            i = j + 1;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

pub fn gen_value_csstree(value: &str) -> String {
    normalize_url_and_strings(&collapse_ws(value.trim()))
}

pub fn gen_value_minify(value: &str) -> String {
    let v = gen_value_csstree(value);
    // CSSO: no space after commas, no space around '/' in font shorthand etc.
    let mut out = String::new();
    let mut quote: Option<char> = None;
    let chars: Vec<char> = v.chars().collect();
    for (idx, c) in chars.iter().enumerate() {
        match quote {
            Some(q) => {
                out.push(*c);
                if *c == q {
                    quote = None;
                }
            }
            None => {
                if *c == '"' || *c == '\'' {
                    quote = Some(*c);
                    out.push(*c);
                } else if *c == ',' {
                    out.push(',');
                    // skip following spaces handled by lookahead below
                    let _ = idx;
                } else if *c == ' '
                    && (out.ends_with(',') || out.ends_with('(')
                        || (idx + 1 < chars.len() && chars[idx + 1] == ')'))
                {
                    // drop
                } else {
                    out.push(*c);
                }
            }
        }
    }
    out
}

pub fn gen_selector(selector: &str) -> String {
    let v = collapse_ws(selector.trim());
    let mut out = String::new();
    let chars: Vec<char> = v.chars().collect();
    let mut quote: Option<char> = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match quote {
            Some(q) => {
                out.push(c);
                if c == q {
                    quote = None;
                }
                i += 1;
            }
            None => {
                if c == '"' || c == '\'' {
                    quote = Some(c);
                    out.push(c);
                    i += 1;
                } else if v[i..].starts_with("/deep/") {
                    out.push_str("/deep/");
                    i += 6;
                    // skip following spaces
                    while i < chars.len() && chars[i] == ' ' {
                        i += 1;
                    }
                    // remove space already emitted before /deep/
                    // (handled: we haven't emitted trailing space specially; trim below)
                } else if matches!(c, '>' | '+' | '~' | ',') {
                    while out.ends_with(' ') {
                        out.pop();
                    }
                    out.push(c);
                    i += 1;
                    while i < chars.len() && chars[i] == ' ' {
                        i += 1;
                    }
                } else if c == ' ' {
                    // keep single descendant space unless around combinator (handled)
                    out.push(' ');
                    i += 1;
                } else if c == '=' {
                    while out.ends_with(' ') {
                        out.pop();
                    }
                    out.push('=');
                    i += 1;
                    while i < chars.len() && chars[i] == ' ' {
                        i += 1;
                    }
                } else {
                    out.push(c);
                    i += 1;
                }
            }
        }
    }
    // remove spaces before /deep/ left by descendant handling
    out.replace(" /deep/", "/deep/").trim().to_string()
}

pub fn gen_prelude(prelude: &str) -> String {
    let v = gen_value_csstree(prelude);
    // remove space after ':' and ',' inside the prelude, and after '('
    let mut out = String::new();
    let chars: Vec<char> = v.chars().collect();
    for (idx, c) in chars.iter().enumerate() {
        if *c == ' '
            && (out.ends_with(':') || out.ends_with(',') || out.ends_with('('))
        {
            continue;
        }
        if *c == ' ' && idx + 1 < chars.len() && chars[idx + 1] == ')' {
            continue;
        }
        out.push(*c);
    }
    out
}

pub fn gen_decl(d: &Decl, minify: bool) -> String {
    let value = if minify { gen_value_minify(&d.value) } else { gen_value_csstree(&d.value) };
    let mut s = format!("{}:{}", d.property, value);
    if d.important {
        s.push_str("!important");
    }
    s
}

pub fn gen_decls(decls: &[Decl], minify: bool) -> String {
    decls.iter().map(|d| gen_decl(d, minify)).collect::<Vec<_>>().join(";")
}

pub fn generate(nodes: &[CssNode], minify: bool) -> String {
    let mut out = String::new();
    for n in nodes {
        match n {
            CssNode::Rule { selectors, decls } => {
                let sels: Vec<String> = selectors.iter().map(|s| gen_selector(s)).collect();
                out.push_str(&sels.join(","));
                out.push('{');
                out.push_str(&gen_decls(decls, minify));
                out.push('}');
            }
            CssNode::Atrule { name, prelude, block } => {
                out.push('@');
                out.push_str(name);
                let pre = gen_prelude(prelude);
                if !pre.is_empty() {
                    out.push(' ');
                    out.push_str(&pre);
                }
                match block {
                    None => out.push(';'),
                    Some(AtruleBlock::Decls(decls)) => {
                        out.push('{');
                        out.push_str(&gen_decls(decls, minify));
                        out.push('}');
                    }
                    Some(AtruleBlock::Rules(inner)) => {
                        out.push('{');
                        out.push_str(&generate(inner, minify));
                        out.push('}');
                    }
                }
            }
        }
    }
    out
}

// ---------- CSSO-like minification helpers ----------
pub fn collapse_longhands(decls: &mut Vec<Decl>) {
    for prefix in ["margin", "padding"] {
        let names = [
            format!("{prefix}-top"),
            format!("{prefix}-right"),
            format!("{prefix}-bottom"),
            format!("{prefix}-left"),
        ];
        let mut idxs: Vec<usize> = Vec::new();
        for n in &names {
            match decls.iter().position(|d| &d.property == n) {
                Some(i) => idxs.push(i),
                None => {
                    idxs.clear();
                    break;
                }
            }
        }
        if idxs.len() != 4 {
            continue;
        }
        let vals: Vec<String> = idxs.iter().map(|&i| gen_value_minify(&decls[i].value)).collect();
        let important = decls[idxs[0]].important;
        if idxs.iter().any(|&i| decls[i].important != important) {
            continue;
        }
        let (t, r, b, l) = (&vals[0], &vals[1], &vals[2], &vals[3]);
        let shorthand = if t == r && r == b && b == l {
            t.clone()
        } else if t == b && r == l {
            format!("{t} {r}")
        } else if r == l {
            format!("{t} {r} {b}")
        } else {
            format!("{t} {r} {b} {l}")
        };
        let first = *idxs.iter().min().unwrap();
        let shorthand_decl = Decl { property: prefix.to_string(), value: shorthand, important };
        let mut sorted = idxs.clone();
        sorted.sort_unstable();
        // remove from highest to lowest, then insert at first
        for &i in sorted.iter().rev() {
            decls.remove(i);
        }
        decls.insert(first, shorthand_decl);
    }
}

pub fn minify_decls(decls: &[Decl]) -> Vec<Decl> {
    let mut out: Vec<Decl> = decls
        .iter()
        .map(|d| Decl {
            property: d.property.clone(),
            value: gen_value_minify(&d.value),
            important: d.important,
        })
        .collect();
    collapse_longhands(&mut out);
    out
}

pub fn minify_block(style: &str) -> String {
    let decls = minify_decls(&parse_decl_list(style));
    gen_decls(&decls, true)
}
