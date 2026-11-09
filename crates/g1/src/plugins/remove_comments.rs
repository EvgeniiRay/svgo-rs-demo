//! Port of `plugins/removeComments.js`.
//! Default `preservePatterns` is `[/^!/]` (keeps `<!--! ... -->`
//! license banners); `preservePatterns: false` removes all comments.

use crate::json::JsonValue;
use crate::xast::{XastChild, XastRoot};

pub fn apply(root: &mut XastRoot, params: &JsonValue) -> Result<(), String> {
    // None => preserve nothing (remove all); Some(list) => preserve matches.
    let patterns: Option<Vec<RegexLite>> = match params.get("preservePatterns") {
        None => Some(vec![RegexLite::new("^!")?]),
        Some(JsonValue::Bool(false)) => None,
        Some(JsonValue::Bool(true)) => {
            return Err(
                "Expected array in removeComments preservePatterns parameter but received true"
                    .into(),
            )
        }
        Some(v @ JsonValue::Array(items)) => {
            let mut out = Vec::new();
            for item in items {
                match item.as_str() {
                    Some(s) => out.push(RegexLite::new(s)?),
                    None => {
                        return Err(format!(
                            "Expected array in removeComments preservePatterns parameter but received {v:?}"
                        ))
                    }
                }
            }
            Some(out)
        }
        Some(other) => {
            return Err(format!(
                "Expected array in removeComments preservePatterns parameter but received {other:?}"
            ))
        }
    };
    strip(&mut root.children, &patterns);
    Ok(())
}

fn strip(children: &mut Vec<XastChild>, patterns: &Option<Vec<RegexLite>>) {
    children.retain(|c| match c {
        XastChild::Comment(value) => match patterns {
            None => false,
            Some(list) => list.iter().any(|re| re.is_match(value)),
        },
        _ => true,
    });
    for child in children.iter_mut() {
        if let XastChild::Element(el) = child {
            strip(&mut el.children, patterns);
        }
    }
}

/// Small regex engine for comment-preserve patterns: supports `^`, `$`,
/// `.`, `*`, `+`, `?`, `[...]`, `[^...]`, escapes (`\s \S \d \D \w \W`
/// and literal escapes), groups `(...)`, `(?:...)` and `a|b`.
/// No backreferences or look-around (documented limitation).
#[derive(Debug, Clone)]
pub struct RegexLite {
    anchored_start: bool,
    alt: Vec<Vec<Piece>>,
}

#[derive(Debug, Clone)]
enum Atom {
    Char(char),
    Any,
    Class { negated: bool, items: Vec<ClassItem> },
    Group(Vec<Vec<Piece>>),
    End,
}

#[derive(Debug, Clone)]
enum ClassItem {
    Ch(char),
    Range(char, char),
    Whitespace(bool),
    Digit(bool),
    Word(bool),
}

#[derive(Debug, Clone)]
struct Piece {
    atom: Atom,
    min: usize,
    max: Option<usize>, // None = unbounded
}

impl RegexLite {
    pub fn new(pattern: &str) -> Result<Self, String> {
        let chars: Vec<char> = pattern.chars().collect();
        let mut pos = 0usize;
        let mut anchored_start = false;
        if chars.first() == Some(&'^') {
            anchored_start = true;
            pos = 1;
        }
        let alt = parse_alt(&chars, &mut pos, false)?;
        if pos != chars.len() {
            return Err(format!("invalid regex pattern: {pattern}"));
        }
        Ok(RegexLite { anchored_start, alt })
    }

    /// Like JS `RegExp.prototype.test`: unanchored search unless `^`.
    pub fn is_match(&self, text: &str) -> bool {
        let chars: Vec<char> = text.chars().collect();
        if self.anchored_start {
            return match_alt(&self.alt, &chars, 0, &|_| true);
        }
        (0..=chars.len()).any(|start| match_alt(&self.alt, &chars, start, &|_| true))
    }
}

fn match_alt(alt: &[Vec<Piece>], text: &[char], pos: usize, cont: &dyn Fn(usize) -> bool) -> bool {
    alt.iter().any(|seq| match_seq(seq, 0, text, pos, cont))
}

fn match_seq(
    pieces: &[Piece],
    idx: usize,
    text: &[char],
    pos: usize,
    cont: &dyn Fn(usize) -> bool,
) -> bool {
    if idx == pieces.len() {
        return cont(pos);
    }
    let piece = &pieces[idx];
    // Positions reachable after min..=max repetitions (longest first).
    let mut ends: Vec<usize> = vec![pos];
    let mut cur = pos;
    let mut count = 0usize;
    loop {
        if piece.max == Some(count) {
            break;
        }
        match match_atom(&piece.atom, text, cur) {
            Some(next) if next != cur || matches!(piece.atom, Atom::End) => {
                count += 1;
                cur = next;
                ends.push(cur);
                if matches!(piece.atom, Atom::End) {
                    break;
                }
            }
            _ => break,
        }
    }
    if count < piece.min {
        return false;
    }
    ends.iter()
        .rev()
        .take_while(|_| true)
        .filter(|_| ends.len() > piece.min)
        .chain(std::iter::empty())
        .any(|&end| {
            // Only try repetition counts >= min.
            let reps = ends.iter().filter(|&&e| e == end).count();
            let _ = reps;
            match_seq(pieces, idx + 1, text, end, cont)
        })
        || (piece.min == 0 && ends.len() == 1 && match_seq(pieces, idx + 1, text, pos, cont))
        || ends
            .iter()
            .enumerate()
            .filter(|(i, _)| *i >= piece.min)
            .rev()
            .any(|(_, &end)| match_seq(pieces, idx + 1, text, end, cont))
}

fn match_atom(atom: &Atom, text: &[char], pos: usize) -> Option<usize> {
    match atom {
        Atom::Char(c) => {
            if text.get(pos) == Some(c) {
                Some(pos + 1)
            } else {
                None
            }
        }
        Atom::Any => text.get(pos).map(|_| pos + 1),
        Atom::End => {
            if pos == text.len() {
                Some(pos)
            } else {
                None
            }
        }
        Atom::Class { negated, items } => {
            let c = *text.get(pos)?;
            let inside = items.iter().any(|item| match item {
                ClassItem::Ch(x) => *x == c,
                ClassItem::Range(a, b) => *a <= c && c <= *b,
                ClassItem::Whitespace(neg) => c.is_whitespace() != *neg,
                ClassItem::Digit(neg) => c.is_ascii_digit() != *neg,
                ClassItem::Word(neg) => (c.is_alphanumeric() || c == '_') != *neg,
            });
            if inside != *negated {
                Some(pos + 1)
            } else {
                None
            }
        }
        Atom::Group(alt) => {
            // Try each alternative; return the first end that matches.
            // (Single-end approximation: group used with test() semantics.)
            for seq in alt {
                let found = std::cell::Cell::new(None);
                let _ = match_seq(seq, 0, text, pos, &|end| {
                    found.set(Some(end));
                    true
                });
                if let Some(end) = found.get() {
                    return Some(end);
                }
            }
            None
        }
    }
}

fn parse_alt(chars: &[char], pos: &mut usize, in_group: bool) -> Result<Vec<Vec<Piece>>, String> {
    let mut alternatives = Vec::new();
    let mut current: Vec<Piece> = Vec::new();
    while let Some(&c) = chars.get(*pos) {
        match c {
            ')' if in_group => break,
            '|' => {
                *pos += 1;
                alternatives.push(std::mem::take(&mut current));
            }
            '(' => {
                *pos += 1;
                if chars.get(*pos) == Some(&'?') && chars.get(*pos + 1) == Some(&':') {
                    *pos += 2;
                }
                let inner = parse_alt(chars, pos, true)?;
                if chars.get(*pos) != Some(&')') {
                    return Err("regex: unterminated group".into());
                }
                *pos += 1;
                current.push(quantified(Atom::Group(inner), chars, pos)?);
            }
            '[' => {
                *pos += 1;
                current.push(quantified(parse_class(chars, pos)?, chars, pos)?);
            }
            '.' => {
                *pos += 1;
                current.push(quantified(Atom::Any, chars, pos)?);
            }
            '$' => {
                *pos += 1;
                current.push(Piece { atom: Atom::End, min: 1, max: Some(1) });
            }
            '\\' => {
                *pos += 1;
                let atom = parse_escape(chars, pos)?;
                current.push(quantified(atom, chars, pos)?);
            }
            '*' | '+' | '?' => return Err("regex: quantifier without atom".into()),
            _ => {
                *pos += 1;
                current.push(quantified(Atom::Char(c), chars, pos)?);
            }
        }
    }
    alternatives.push(current);
    Ok(alternatives)
}

fn quantified(atom: Atom, chars: &[char], pos: &mut usize) -> Result<Piece, String> {
    let (min, max) = match chars.get(*pos) {
        Some('*') => {
            *pos += 1;
            (0, None)
        }
        Some('+') => {
            *pos += 1;
            (1, None)
        }
        Some('?') => {
            *pos += 1;
            (0, Some(1))
        }
        Some('{') => {
            // {n}, {n,}, {n,m}
            let mut j = *pos + 1;
            let mut lo = String::new();
            while matches!(chars.get(j), Some(c) if c.is_ascii_digit()) {
                lo.push(chars[j]);
                j += 1;
            }
            let mut hi: Option<Option<usize>> = None;
            if chars.get(j) == Some(&',') {
                j += 1;
                let mut h = String::new();
                while matches!(chars.get(j), Some(c) if c.is_ascii_digit()) {
                    h.push(chars[j]);
                    j += 1;
                }
                hi = Some(if h.is_empty() { None } else { Some(h.parse().map_err(|_| "regex: bad quantifier")?) });
            }
            if chars.get(j) != Some(&'}') || lo.is_empty() {
                // Not a quantifier: treat '{' literally by not consuming.
                return Ok(Piece { atom, min: 1, max: Some(1) });
            }
            let lo_n: usize = lo.parse().map_err(|_| "regex: bad quantifier")?;
            *pos = j + 1;
            match hi {
                None => (lo_n, Some(lo_n)),
                Some(h) => (lo_n, h),
            }
        }
        _ => (1, Some(1)),
    };
    if chars.get(*pos) == Some(&'?') {
        *pos += 1; // lazy quantifier: same existence semantics
    }
    Ok(Piece { atom, min, max })
}

fn parse_class(chars: &[char], pos: &mut usize) -> Result<Atom, String> {
    let mut negated = false;
    if chars.get(*pos) == Some(&'^') {
        negated = true;
        *pos += 1;
    }
    let mut items = Vec::new();
    let mut first = true;
    loop {
        match chars.get(*pos) {
            None => return Err("regex: unterminated character class".into()),
            Some(']') if !first => {
                *pos += 1;
                return Ok(Atom::Class { negated, items });
            }
            Some(_) => {
                first = false;
                let lo = parse_class_char(chars, pos, &mut items)?;
                if let Some(c) = lo {
                    if chars.get(*pos) == Some(&'-')
                        && !matches!(chars.get(*pos + 1), None | Some(']'))
                    {
                        *pos += 1;
                        let hi_items_mark = items.len();
                        let hi = parse_class_char(chars, pos, &mut items)?;
                        let _ = hi_items_mark;
                        match hi {
                            Some(h) => items.push(ClassItem::Range(c, h)),
                            None => items.push(ClassItem::Ch(c)),
                        }
                    } else {
                        items.push(ClassItem::Ch(c));
                    }
                }
            }
        }
    }
}

/// Returns a plain char, or None after pushing a class-escape item.
fn parse_class_char(
    chars: &[char],
    pos: &mut usize,
    items: &mut Vec<ClassItem>,
) -> Result<Option<char>, String> {
    match chars.get(*pos) {
        Some('\\') => {
            *pos += 1;
            match parse_escape(chars, pos)? {
                Atom::Char(c) => Ok(Some(c)),
                Atom::Class { negated, items: inner } => {
                    for it in inner {
                        items.push(match it {
                            ClassItem::Whitespace(n) => ClassItem::Whitespace(n ^ negated),
                            ClassItem::Digit(n) => ClassItem::Digit(n ^ negated),
                            ClassItem::Word(n) => ClassItem::Word(n ^ negated),
                            other => other,
                        });
                    }
                    Ok(None)
                }
                _ => Err("regex: bad class escape".into()),
            }
        }
        Some(c) => {
            let c = *c;
            *pos += 1;
            Ok(Some(c))
        }
        None => Err("regex: unterminated character class".into()),
    }
}

fn parse_escape(chars: &[char], pos: &mut usize) -> Result<Atom, String> {
    match chars.get(*pos) {
        None => Err("regex: trailing backslash".into()),
        Some(c) => {
            let c = *c;
            *pos += 1;
            Ok(match c {
                's' => Atom::Class { negated: false, items: vec![ClassItem::Whitespace(false)] },
                'S' => Atom::Class { negated: false, items: vec![ClassItem::Whitespace(true)] },
                'd' => Atom::Class { negated: false, items: vec![ClassItem::Digit(false)] },
                'D' => Atom::Class { negated: false, items: vec![ClassItem::Digit(true)] },
                'w' => Atom::Class { negated: false, items: vec![ClassItem::Word(false)] },
                'W' => Atom::Class { negated: false, items: vec![ClassItem::Word(true)] },
                'n' => Atom::Char('\n'),
                'r' => Atom::Char('\r'),
                't' => Atom::Char('\t'),
                other => Atom::Char(other),
            })
        }
    }
}
