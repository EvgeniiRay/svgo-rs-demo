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
                    && (out.ends_with(',') || out.ends_with('('))
                {
                    // drop
                } else if *c == ' ' && idx + 1 < chars.len() && chars[idx + 1] == ')' {
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
                let pre = if minify { gen_prelude(prelude) } else { gen_prelude(prelude) };
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

const NAME_TO_HEX: &str = "aliceblue:f0f8ff,antiquewhite:faebd7,aqua:0ff,aquamarine:7fffd4,azure:f0ffff,beige:f5f5dc,bisque:ffe4c4,black:000,blanchedalmond:ffebcd,blue:00f,blueviolet:8a2be2,brown:a52a2a,burlywood:deb887,cadetblue:5f9ea0,chartreuse:7fff00,chocolate:d2691e,coral:ff7f50,cornflowerblue:6495ed,cornsilk:fff8dc,crimson:dc143c,cyan:0ff,darkblue:00008b,darkcyan:008b8b,darkgoldenrod:b8860b,darkgray:a9a9a9,darkgrey:a9a9a9,darkgreen:006400,darkkhaki:bdb76b,darkmagenta:8b008b,darkolivegreen:556b2f,darkorange:ff8c00,darkorchid:9932cc,darkred:8b0000,darksalmon:e9967a,darkseagreen:8fbc8f,darkslateblue:483d8b,darkslategray:2f4f4f,darkslategrey:2f4f4f,darkturquoise:00ced1,darkviolet:9400d3,deeppink:ff1493,deepskyblue:00bfff,dimgray:696969,dimgrey:696969,dodgerblue:1e90ff,firebrick:b22222,floralwhite:fffaf0,forestgreen:228b22,fuchsia:f0f,gainsboro:dcdcdc,ghostwhite:f8f8ff,gold:ffd700,goldenrod:daa520,gray:808080,grey:808080,green:008000,greenyellow:adff2f,honeydew:f0fff0,hotpink:ff69b4,indianred:cd5c5c,indigo:4b0082,ivory:fffff0,khaki:f0e68c,lavender:e6e6fa,lavenderblush:fff0f5,lawngreen:7cfc00,lemonchiffon:fffacd,lightblue:add8e6,lightcoral:f08080,lightcyan:e0ffff,lightgoldenrodyellow:fafad2,lightgray:d3d3d3,lightgrey:d3d3d3,lightgreen:90ee90,lightpink:ffb6c1,lightsalmon:ffa07a,lightseagreen:20b2aa,lightskyblue:87cefa,lightslategray:789,lightslategrey:789,lightsteelblue:b0c4de,lightyellow:ffffe0,lime:0f0,limegreen:32cd32,linen:faf0e6,magenta:f0f,maroon:800000,mediumaquamarine:66cdaa,mediumblue:0000cd,mediumorchid:ba55d3,mediumpurple:9370db,mediumseagreen:3cb371,mediumslateblue:7b68ee,mediumspringgreen:00fa9a,mediumturquoise:48d1cc,mediumvioletred:c71585,midnightblue:191970,mintcream:f5fffa,mistyrose:ffe4e1,moccasin:ffe4b5,navajowhite:ffdead,navy:000080,oldlace:fdf5e6,olive:808000,olivedrab:6b8e23,orange:ffa500,orangered:ff4500,orchid:da70d6,palegoldenrod:eee8aa,palegreen:98fb98,paleturquoise:afeeee,palevioletred:db7093,papayawhip:ffefd5,peachpuff:ffdab9,peru:cd853f,pink:ffc0cb,plum:dda0dd,powderblue:b0e0e6,purple:800080,rebeccapurple:639,red:f00,rosybrown:bc8f8f,royalblue:4169e1,saddlebrown:8b4513,salmon:fa8072,sandybrown:f4a460,seagreen:2e8b57,seashell:fff5ee,sienna:a0522d,silver:c0c0c0,skyblue:87ceeb,slateblue:6a5acd,slategray:708090,slategrey:708090,snow:fffafa,springgreen:00ff7f,steelblue:4682b4,tan:d2b48c,teal:008080,thistle:d8bfd8,tomato:ff6347,turquoise:40e0d0,violet:ee82ee,wheat:f5deb3,white:fff,whitesmoke:f5f5f5,yellow:ff0,yellowgreen:9acd32";
const HEX_TO_NAME: &str = "800000:maroon,800080:purple,808000:olive,808080:gray,00ffff:cyan,f0ffff:azure,f5f5dc:beige,ffe4c4:bisque,000000:black,0000ff:blue,a52a2a:brown,ff7f50:coral,ffd700:gold,008000:green,4b0082:indigo,fffff0:ivory,f0e68c:khaki,00ff00:lime,faf0e6:linen,000080:navy,ffa500:orange,da70d6:orchid,cd853f:peru,ffc0cb:pink,dda0dd:plum,f00:red,ff0000:red,fa8072:salmon,a0522d:sienna,c0c0c0:silver,fffafa:snow,d2b48c:tan,008080:teal,ff6347:tomato,ee82ee:violet,f5deb3:wheat,ffffff:white,ffff00:yellow";

fn table_lookup(table: &str, key: &str) -> Option<String> {
    table.split(',').find_map(|e| {
        let (k, v) = e.split_once(':')?;
        if k == key { Some(v.to_string()) } else { None }
    })
}

pub fn pack_number(value: &str) -> String {
    let v = value.trim();
    let (sign, rest) = if let Some(r) = v.strip_prefix('-') {
        ("-", r)
    } else if let Some(r) = v.strip_prefix('+') {
        ("", r)
    } else {
        ("", v)
    };
    let (int_part, frac_part) = match rest.split_once('.') {
        Some((a, b)) => (a, b),
        None => (rest, ""),
    };
    let int_trim = int_part.trim_start_matches('0');
    let frac_trim = frac_part.trim_end_matches('0');
    if frac_trim.is_empty() {
        if int_trim.is_empty() {
            return "0".to_string();
        }
        return format!("{sign}{int_trim}");
    }
    if int_trim.is_empty() {
        return format!("{sign}.{frac_trim}");
    }
    format!("{sign}{int_trim}.{frac_trim}")
}

fn compress_hex_token(hex: &str) -> String {
    let mut color = hex.to_lowercase();
    if color.len() == 6
        && color.as_bytes()[0] == color.as_bytes()[1]
        && color.as_bytes()[2] == color.as_bytes()[3]
        && color.as_bytes()[4] == color.as_bytes()[5]
    {
        color = format!(
            "{}{}{}",
            color.as_bytes()[0] as char,
            color.as_bytes()[2] as char,
            color.as_bytes()[4] as char
        );
    }
    if let Some(name) = table_lookup(HEX_TO_NAME, &color) {
        // csso replaces unconditionally; all mapped names are no longer
        // than the #-form they replace after shortening above
        return name;
    }
    format!("#{color}")
}

fn parse_color_args(inner: &str) -> Option<Vec<String>> {
    let inner = inner.trim().trim_end_matches(',');
    let mut args = Vec::new();
    for part in inner.split(|c: char| c == ',' || c.is_whitespace() || c == '/') {
        if !part.is_empty() {
            args.push(part.to_string());
        }
    }
    if args.is_empty() { None } else { Some(args) }
}

fn arg_byte(arg: &str) -> Option<f64> {
    if let Some(p) = arg.strip_suffix('%') {
        let n: f64 = p.parse().ok()?;
        Some((n.clamp(0.0, 100.0) * 2.55).round())
    } else {
        let n: f64 = arg.parse().ok()?;
        Some(n.clamp(0.0, 255.0).round())
    }
}

fn hsl_to_hex(h_deg: f64, s_pct: f64, l_pct: f64) -> String {
    let h = (((h_deg % 360.0) + 360.0) % 360.0) / 360.0;
    let s = (s_pct / 100.0).clamp(0.0, 1.0);
    let l = (l_pct / 100.0).clamp(0.0, 1.0);
    let (r, g, b) = if s == 0.0 {
        (l, l, l)
    } else {
        let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
        let p = 2.0 * l - q;
        let hue = |mut t: f64| {
            if t < 0.0 { t += 1.0; }
            if t > 1.0 { t -= 1.0; }
            if t < 1.0 / 6.0 { p + (q - p) * 6.0 * t } else if t < 0.5 { q } else if t < 2.0 / 3.0 { p + (q - p) * (2.0 / 3.0 - t) * 6.0 } else { p }
        };
        (hue(h + 1.0 / 3.0), hue(h), hue(h - 1.0 / 3.0))
    };
    let hex = format!(
        "{:02x}{:02x}{:02x}",
        (r * 255.0).round() as i64,
        (g * 255.0).round() as i64,
        (b * 255.0).round() as i64
    );
    compress_hex_token(&hex)
}

/// CSSO `replace` stage for a declaration value: colour compression,
/// number packing and zero-length removal, outside strings and url().
fn compress_decl_value(property: &str, value: &str) -> String {
    let prop = property.to_lowercase();
    let base_prop = prop.rsplit('-').next().unwrap_or(&prop).to_string();
    let _ = &base_prop;
    if (prop == "border" || prop.starts_with("border-") || prop == "outline" || prop.starts_with("outline-"))
        && value.trim().eq_ignore_ascii_case("none")
    {
        return "0".to_string();
    }
    if prop == "font-weight" {
        match value.trim().to_lowercase().as_str() {
            "bold" => return "700".to_string(),
            "normal" => return "400".to_string(),
            _ => {}
        }
    }
    let chars: Vec<char> = value.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    let mut fn_stack: Vec<String> = Vec::new();
    let no_color_ident = matches!(
        prop.as_str(),
        "font-family" | "src" | "content" | "animation-name" | "font" | "unicode-range"
    );
    while i < chars.len() {
        let c = chars[i];
        // strings verbatim
        if c == '"' || c == '\'' {
            let q = c;
            out.push(c);
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                    out.push(chars[i]);
                } else if chars[i] == q {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        // url(...) verbatim
        if (c == 'u' || c == 'U')
            && value[chars[..i].iter().collect::<String>().len()..]
                .to_lowercase()
                .starts_with("url(")
        {
            let mut depth = 0i32;
            while i < chars.len() {
                let ch = chars[i];
                out.push(ch);
                i += 1;
                if ch == '(' { depth += 1; } else if ch == ')' {
                    depth -= 1;
                    if depth == 0 { break; }
                }
            }
            continue;
        }
        if c == '(' {
            fn_stack.push(String::new());
            out.push(c);
            i += 1;
            continue;
        }
        if c == ')' {
            fn_stack.pop();
            out.push(c);
            i += 1;
            continue;
        }
        if c == '#' {
            let mut j = i + 1;
            let mut hex = String::new();
            while j < chars.len() && chars[j].is_ascii_hexdigit() {
                hex.push(chars[j]);
                j += 1;
            }
            if matches!(hex.len(), 3 | 4 | 6 | 8) {
                if hex.len() <= 6 {
                    out.push_str(&compress_hex_token(&hex));
                } else {
                    out.push('#');
                    out.push_str(&hex.to_lowercase());
                }
                i = j;
                continue;
            }
            out.push(c);
            i += 1;
            continue;
        }
        // number / dimension / percentage
        let num_start = c.is_ascii_digit()
            || (c == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
            || ((c == '-' || c == '+')
                && i + 1 < chars.len()
                && (chars[i + 1].is_ascii_digit()
                    || (chars[i + 1] == '.' && i + 2 < chars.len() && chars[i + 2].is_ascii_digit())));
        if num_start {
            let mut j = i;
            let mut num = String::new();
            if chars[j] == '-' || chars[j] == '+' {
                num.push(chars[j]);
                j += 1;
            }
            let mut seen_dot = false;
            while j < chars.len() && (chars[j].is_ascii_digit() || (chars[j] == '.' && !seen_dot)) {
                if chars[j] == '.' { seen_dot = true; }
                num.push(chars[j]);
                j += 1;
            }
            let mut unit = String::new();
            while j < chars.len() && (chars[j].is_ascii_alphabetic() || chars[j] == '%') {
                unit.push(chars[j]);
                j += 1;
            }
            let packed = pack_number(&num);
            let in_math = fn_stack.iter().any(|f| matches!(f.as_str(), "calc" | "min" | "max" | "clamp"));
            let length_units = [
                "px", "mm", "cm", "in", "pt", "pc", "em", "ex", "ch", "rem", "vh", "vw", "vmin",
                "vmax", "vm",
            ];
            if packed == "0"
                && unit != "%"
                && !unit.is_empty()
                && length_units.contains(&unit.to_lowercase().as_str())
                && !in_math
                && prop != "flex"
                && prop != "-ms-flex"
            {
                out.push('0');
            } else if packed == "0" && unit == "%" && !matches!(prop.as_str(), "width" | "min-width" | "max-width" | "height" | "min-height" | "max-height" | "flex" | "-ms-flex") && !in_math {
                // csso converts 0% to 0 only where a length is valid; keep
                // the conversion conservative outside known SVG lengths
                out.push_str("0%");
            } else {
                out.push_str(&packed);
                out.push_str(&unit);
            }
            i = j;
            continue;
        }
        // identifier / function
        if c.is_ascii_alphabetic() || c == '_' || c == '-' {
            let mut j = i;
            let mut ident = String::new();
            while j < chars.len()
                && (chars[j].is_ascii_alphanumeric() || chars[j] == '-' || chars[j] == '_')
            {
                ident.push(chars[j]);
                j += 1;
            }
            let lower = ident.to_lowercase();
            if j < chars.len() && chars[j] == '(' {
                // colour functions
                if matches!(lower.as_str(), "rgb" | "rgba" | "hsl" | "hsla") {
                    // find matching close
                    let mut depth = 0i32;
                    let mut k = j;
                    let mut inner = String::new();
                    while k < chars.len() {
                        let ch = chars[k];
                        if ch == '(' {
                            depth += 1;
                            if depth > 1 { inner.push(ch); }
                        } else if ch == ')' {
                            depth -= 1;
                            if depth == 0 { break; }
                            inner.push(ch);
                        } else {
                            inner.push(ch);
                        }
                        k += 1;
                    }
                    if k < chars.len() {
                        if let Some(args) = parse_color_args(&inner) {
                            let converted: Option<String> = match lower.as_str() {
                                "rgb" if args.len() == 3 => {
                                    let b: Option<Vec<f64>> =
                                        args.iter().map(|a| arg_byte(a)).collect();
                                    b.map(|b| {
                                        compress_hex_token(&format!(
                                            "{:02x}{:02x}{:02x}",
                                            b[0] as i64, b[1] as i64, b[2] as i64
                                        ))
                                    })
                                }
                                "rgba" if args.len() == 4 => {
                                    let alpha: Option<f64> = args[3]
                                        .strip_suffix('%')
                                        .map(|p| p.parse::<f64>().map(|v| v / 100.0))
                                        .unwrap_or_else(|| args[3].parse::<f64>())
                                        .ok();
                                    match alpha {
                                        Some(1.0) => {
                                            let b: Option<Vec<f64>> =
                                                args[..3].iter().map(|a| arg_byte(a)).collect();
                                            b.map(|b| {
                                                compress_hex_token(&format!(
                                                    "{:02x}{:02x}{:02x}",
                                                    b[0] as i64, b[1] as i64, b[2] as i64
                                                ))
                                            })
                                        }
                                        Some(0.0) => {
                                            let b: Option<Vec<f64>> =
                                                args[..3].iter().map(|a| arg_byte(a)).collect();
                                            match b {
                                                Some(b) if b.iter().all(|v| *v == 0.0) => {
                                                    Some("transparent".to_string())
                                                }
                                                _ => None,
                                            }
                                        }
                                        Some(a) => {
                                            let b: Option<Vec<f64>> =
                                                args[..3].iter().map(|x| arg_byte(x)).collect();
                                            b.map(|b| {
                                                format!(
                                                    "rgba({},{},{},{})",
                                                    b[0] as i64,
                                                    b[1] as i64,
                                                    b[2] as i64,
                                                    pack_number(&a.to_string())
                                                )
                                            })
                                        }
                                        None => None,
                                    }
                                }
                                "hsl" if args.len() == 3 => {
                                    let h: Option<f64> =
                                        args[0].trim_end_matches("deg").parse().ok();
                                    let s: Option<f64> =
                                        args[1].strip_suffix('%').and_then(|p| p.parse().ok());
                                    let l: Option<f64> =
                                        args[2].strip_suffix('%').and_then(|p| p.parse().ok());
                                    match (h, s, l) {
                                        (Some(h), Some(s), Some(l)) => Some(hsl_to_hex(h, s, l)),
                                        _ => None,
                                    }
                                }
                                "hsla" if args.len() == 4 => {
                                    let h: Option<f64> =
                                        args[0].trim_end_matches("deg").parse().ok();
                                    let s: Option<f64> =
                                        args[1].strip_suffix('%').and_then(|p| p.parse().ok());
                                    let l: Option<f64> =
                                        args[2].strip_suffix('%').and_then(|p| p.parse().ok());
                                    let alpha: Option<f64> = args[3].parse().ok();
                                    match (h, s, l, alpha) {
                                        (Some(h), Some(s), Some(l), Some(1.0)) => {
                                            Some(hsl_to_hex(h, s, l))
                                        }
                                        _ => None,
                                    }
                                }
                                _ => None,
                            };
                            if let Some(conv) = converted {
                                out.push_str(&conv);
                                i = k + 1;
                                continue;
                            }
                        }
                    }
                }
                if let Some(top) = fn_stack.last_mut() {
                    *top = lower.clone();
                }
                // ordinary function: emit name, let '(' be handled next loop
                out.push_str(&ident);
                i = j;
                continue;
            }
            // plain identifier: colour name compression
            if prop == "font" && lower == "bold" {
                out.push_str("700");
                i = j;
                continue;
            }
            if !no_color_ident {
                if let Some(hex) = table_lookup(NAME_TO_HEX, &lower) {
                    if hex.len() + 1 <= lower.len() {
                        out.push('#');
                        out.push_str(&hex);
                        i = j;
                        continue;
                    } else {
                        out.push_str(if lower == "grey" { "gray" } else { &lower });
                        i = j;
                        continue;
                    }
                }
            }
            out.push_str(&ident);
            i = j;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

fn dedup_decls(decls: Vec<Decl>) -> Vec<Decl> {
    // csso restructBlock: a later declaration of the same property wins,
    // unless either value is complex (functions / lists can be fallbacks)
    let simple = |v: &str| !v.contains('(') && !v.contains(',');
    let mut kept: Vec<Decl> = Vec::new();
    for decl in decls {
        if simple(&decl.value) {
            if let Some(pos) = kept.iter().position(|d: &Decl| {
                d.property.to_lowercase() == decl.property.to_lowercase()
                    && d.important == decl.important
                    && simple(&d.value)
            }) {
                // keep position of the later declaration (csso removes the
                // earlier one when scanning right-to-left)
                kept.remove(pos);
            } else if decl.important {
                if let Some(pos) = kept.iter().position(|d: &Decl| {
                    d.property.to_lowercase() == decl.property.to_lowercase() && !d.important && simple(&d.value)
                }) {
                    kept.remove(pos);
                }
            }
        }
        kept.push(decl);
    }
    kept
}

pub fn minify_decls(decls: &[Decl]) -> Vec<Decl> {
    let mut out: Vec<Decl> = decls
        .iter()
        .map(|d| Decl {
            property: d.property.clone(),
            value: gen_value_minify(&compress_decl_value(&d.property, &d.value)),
            important: d.important,
        })
        .collect();
    out = dedup_decls(out);
    collapse_longhands(&mut out);
    out
}

pub fn minify_block(style: &str) -> String {
    let decls = minify_decls(&parse_decl_list(style));
    gen_decls(&decls, true)
}
