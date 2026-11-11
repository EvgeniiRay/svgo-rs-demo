use crate::collections::{inheritable, presentation, presentation_non_inheritable_group};
use crate::xast::{Element, Node, Root};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Declaration { pub name: String, pub value: String, pub important: bool }
#[derive(Debug, Clone)]
pub struct Rule { pub selector: String, pub specificity: [i64;4], pub dynamic: bool, pub declarations: Vec<Declaration> }
#[derive(Debug, Clone, Default)]
pub struct Stylesheet { pub rules: Vec<Rule> }
#[derive(Debug, Clone, PartialEq)]
pub enum Computed { Static(String), Dynamic }
impl Computed { pub fn static_value(&self) -> Option<&str> { if let Computed::Static(v)=self {Some(v)} else {None} } }

fn parse_declarations(css: &str) -> Vec<Declaration> {
    let mut out = Vec::new();
    for part in css.split(';') {
        let part = part.trim();
        if let Some((name, mut value)) = part.split_once(':') {
            let name = name.trim().to_string();
            let mut important = false;
            let vtrim = value.trim();
            let value_str;
            if let Some(stripped) = vtrim.strip_suffix("!important") { important = true; value_str = stripped.trim().to_string(); }
            else { value_str = vtrim.to_string(); }
            let _ = &mut value; 
            if !name.is_empty() { out.push(Declaration { name, value: value_str, important }); }
        }
    }
    out
}
pub fn parse_style_declarations(css: &str) -> Vec<Declaration> { parse_declarations(css) }

fn strip_pseudo(sel: &str) -> (String, bool) {
    // remove :pseudo and ::pseudo (with optional (...)), mark dynamic if any
    let mut out = String::new(); let mut dynamic = false;
    let chars: Vec<char> = sel.chars().collect(); let mut i=0;
    while i < chars.len() {
        if chars[i]==':' {
            dynamic = true;
            // skip colons and name and optional parens
            while i < chars.len() && chars[i]==':' { i+=1; }
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i]=='-' || chars[i]=='_') { i+=1; }
            if i < chars.len() && chars[i]=='(' { let mut d=0; while i<chars.len() { if chars[i]=='(' {d+=1;} else if chars[i]==')' {d-=1; if d==0 {i+=1; break;}} i+=1; } }
            continue;
        }
        out.push(chars[i]); i+=1;
    }
    (out.trim().to_string(), dynamic)
}
fn specificity(orig: &str) -> [i64;4] {
    let mut a=0; let mut b=0; let mut c=0;
    let chars: Vec<char> = orig.chars().collect(); let mut i=0;
    while i<chars.len() {
        match chars[i] {
            '#' => { a+=1; i+=1; while i<chars.len() && (chars[i].is_alphanumeric()||chars[i]=='-'||chars[i]=='_') {i+=1;} }
            '.' => { b+=1; i+=1; while i<chars.len() && (chars[i].is_alphanumeric()||chars[i]=='-'||chars[i]=='_') {i+=1;} }
            '[' => { b+=1; while i<chars.len() && chars[i]!=']' {i+=1;} if i<chars.len() {i+=1;} }
            ':' => { b+=1; while i<chars.len() && chars[i]==':' {i+=1;} while i<chars.len() && (chars[i].is_alphanumeric()||chars[i]=='-'||chars[i]=='_') {i+=1;} if i<chars.len() && chars[i]=='(' { let mut d=0; while i<chars.len() { if chars[i]=='(' {d+=1;} else if chars[i]==')' {d-=1; if d==0 {i+=1;break;}} i+=1; } } }
            '*' => { i+=1; }
            ch if ch.is_alphabetic() => { c+=1; while i<chars.len() && (chars[i].is_alphanumeric()||chars[i]=='-'||chars[i]=='_') {i+=1;} }
            _ => { i+=1; }
        }
    }
    [0,a,b,c]
}
fn split_selectors(s: &str) -> Vec<String> {
    let mut out=Vec::new(); let mut cur=String::new(); let mut d=0;
    for ch in s.chars() { match ch { '('|'[' => {d+=1; cur.push(ch);} ')'|']' => {d-=1; cur.push(ch);} ',' if d==0 => { out.push(cur.trim().to_string()); cur=String::new(); } _=>cur.push(ch) } }
    if !cur.trim().is_empty() { out.push(cur.trim().to_string()); }
    out
}
fn parse_stylesheet(css: &str, dynamic: bool, rules: &mut Vec<Rule>) {
    let css = {
        // strip /* */ comments
        let mut o=String::new(); let mut chars=css.chars().peekable();
        while let Some(c)=chars.next() {
            if c=='/' && chars.peek()==Some(&'*') { chars.next(); let mut prev=' '; for cc in chars.by_ref() { if prev=='*' && cc=='/' { break; } prev=cc; } }
            else { o.push(c); }
        } o
    };
    let b = css.as_bytes(); let mut i=0;
    while i < b.len() {
        // find next '{'
        let open = match css[i..].find('{') { Some(o)=>i+o, None=>break };
        let prelude = css[i..open].trim().to_string();
        // find matching '}'
        let mut depth=0; let mut j=open;
        while j < b.len() { if b[j]==b'{' {depth+=1;} else if b[j]==b'}' {depth-=1; if depth==0 {break;}} j+=1; }
        if j>=b.len() { break; }
        let block = &css[open+1..j];
        if let Some(stripped) = prelude.strip_prefix('@') {
            let name = stripped.split(|c:char| c.is_whitespace()||c=='(').next().unwrap_or("");
            if !matches!(name, "keyframes"|"-webkit-keyframes"|"-o-keyframes"|"-moz-keyframes") {
                parse_stylesheet(block, true, rules);
            }
        } else if !prelude.is_empty() {
            let decls = parse_declarations(block);
            for sel in split_selectors(&prelude) {
                let spec = specificity(&sel);
                let (clean, has_pseudo) = strip_pseudo(&sel);
                rules.push(Rule { selector: clean, specificity: spec, dynamic: dynamic||has_pseudo, declarations: decls.clone() });
            }
        }
        i = j+1;
    }
}
pub fn collect_stylesheet(root: &Root) -> Stylesheet {
    let mut rules = Vec::new();
    fn walk(children: &[Node], rules: &mut Vec<Rule>) {
        for c in children {
            if let Node::Element(e)=c {
                if e.name=="style" {
                    let t = e.get("type");
                    if t.is_none_or(|v| v.is_empty() || v=="text/css") {
                        let dynamic = e.get("media").is_some_and(|m| m!="all");
                        for ch in &e.children { if let Node::Text(t)|Node::CData(t)=ch { parse_stylesheet(t, dynamic, rules); } }
                    }
                }
                walk(&e.children, rules);
            }
        }
    }
    walk(&root.children, &mut rules);
    rules.sort_by_key(|a| a.specificity);
    Stylesheet { rules }
}

// selector matching
#[derive(Debug, Clone)]
struct Compound { tag: Option<String>, id: Option<String>, classes: Vec<String>, attrs: Vec<(String, Option<String>, String)> }
fn parse_compound(s: &str) -> Compound {
    let mut comp = Compound { tag: None, id: None, classes: vec![], attrs: vec![] };
    let chars: Vec<char> = s.chars().collect(); let mut i=0;
    // tag
    if i<chars.len() && (chars[i].is_alphabetic() || chars[i]=='*') {
        let st=i; while i<chars.len() && (chars[i].is_alphanumeric()||chars[i]=='-'||chars[i]=='_'||chars[i]=='*') {i+=1;}
        let t: String = chars[st..i].iter().collect();
        if t!="*" { comp.tag=Some(t); }
    }
    while i<chars.len() {
        match chars[i] {
            '.' => { i+=1; let st=i; while i<chars.len() && (chars[i].is_alphanumeric()||chars[i]=='-'||chars[i]=='_') {i+=1;} comp.classes.push(chars[st..i].iter().collect()); }
            '#' => { i+=1; let st=i; while i<chars.len() && (chars[i].is_alphanumeric()||chars[i]=='-'||chars[i]=='_') {i+=1;} comp.id=Some(chars[st..i].iter().collect()); }
            '[' => { i+=1; let st=i; while i<chars.len() && chars[i]!=']' {i+=1;} let inner: String = chars[st..i].iter().collect(); if i<chars.len() {i+=1;}
                // parse name op value
                let ops = ["~=","|=","^=","$=","*=","="];
                let mut parsed=None;
                for op in ops { if let Some(p)=inner.find(op) { let name=inner[..p].trim().to_string(); let mut val=inner[p+op.len()..].trim().to_string(); if (val.starts_with('"')&&val.ends_with('"'))||(val.starts_with('\'')&&val.ends_with('\'')) { val=val[1..val.len()-1].to_string(); } parsed=Some((name, Some(val), op.to_string())); break; } }
                if let Some(p)=parsed { comp.attrs.push(p); } else { comp.attrs.push((inner.trim().to_string(), None, "=".into())); }
            }
            _ => { i+=1; }
        }
    }
    comp
}
fn compound_matches(comp: &Compound, el: &Element) -> bool {
    if let Some(t)=&comp.tag { if &el.name!=t { return false; } }
    if let Some(id)=&comp.id { if el.get("id")!=Some(id.as_str()) { return false; } }
    for cl in &comp.classes {
        let cls = el.get("class").unwrap_or("");
        if !cls.split_whitespace().any(|c| c==cl) { return false; }
    }
    for (name, val, op) in &comp.attrs {
        let Some(actual) = el.get(name) else { return false };
        if let Some(v)=val {
            let ok = match op.as_str() {
                "=" => actual==v, "~=" => actual.split_whitespace().any(|x| x==v), "|=" => actual==v || actual.starts_with(&format!("{}-",v)),
                "^=" => actual.starts_with(v.as_str()), "$=" => actual.ends_with(v.as_str()), "*=" => actual.contains(v.as_str()), _=>actual==v,
            };
            if !ok { return false; }
        }
    }
    true
}
pub fn selector_matches(selector: &str, el: &Element, ancestors: &[&Element]) -> bool {
    // split into compounds by combinators; support descendant and child (>), treat +/~ as descendant approx
    let mut compounds: Vec<Compound> = Vec::new();
    let mut combinators: Vec<char> = Vec::new(); // between compounds: ' ' or '>'
    let mut cur = String::new(); let mut depth=0;
    let mut pending: Option<char> = None;
    for ch in selector.chars() {
        match ch {
            '['|'(' => {depth+=1; cur.push(ch);}
            ']'|')' => {depth-=1; cur.push(ch);}
            '>'|'+'|'~' if depth==0 => { if !cur.trim().is_empty() { compounds.push(parse_compound(cur.trim())); cur=String::new(); } pending=Some(if ch=='>' {'>'} else {' '}); }
            c if c.is_whitespace() && depth==0 => { if !cur.trim().is_empty() { compounds.push(parse_compound(cur.trim())); cur=String::new(); if pending.is_none() { pending=Some(' '); } } }
            _ => {
                if let Some(p)=pending.take() { if !compounds.is_empty() && combinators.len() < compounds.len() { combinators.push(p); } }
                cur.push(ch);
            }
        }
    }
    if !cur.trim().is_empty() { compounds.push(parse_compound(cur.trim())); }
    if compounds.is_empty() { return false; }
    // match last against el, then walk ancestors
    if !compound_matches(compounds.last().unwrap(), el) { return false; }
    let mut anc_idx: i32 = ancestors.len() as i32 - 1;
    for ci in (0..compounds.len()-1).rev() {
        let comb = if ci < combinators.len() { combinators[ci] } else { ' ' };
        if comb=='>' {
            if anc_idx < 0 { return false; }
            if !compound_matches(&compounds[ci], ancestors[anc_idx as usize]) { return false; }
            anc_idx-=1;
        } else {
            let mut found=false;
            while anc_idx>=0 { if compound_matches(&compounds[ci], ancestors[anc_idx as usize]) { found=true; anc_idx-=1; break; } anc_idx-=1; }
            if !found { return false; }
        }
    }
    true
}
fn compute_own(stylesheet: &Stylesheet, node: &Element, ancestors: &[&Element]) -> HashMap<String, Computed> {
    let mut computed: HashMap<String, Computed> = HashMap::new();
    let mut important_map: HashMap<String, bool> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let set = |computed: &mut HashMap<String,Computed>, order: &mut Vec<String>, name: &str, val: Computed| {
        if !computed.contains_key(name) { order.push(name.to_string()); }
        computed.insert(name.to_string(), val);
    };
    for (name, value) in &node.attributes {
        if presentation(name) { set(&mut computed, &mut order, name, Computed::Static(value.clone())); important_map.insert(name.clone(), false); }
    }
    for rule in &stylesheet.rules {
        if selector_matches(&rule.selector, node, ancestors) {
            for decl in &rule.declarations {
                if matches!(computed.get(&decl.name), Some(Computed::Dynamic)) { continue; }
                if rule.dynamic { set(&mut computed, &mut order, &decl.name, Computed::Dynamic); continue; }
                let should = !computed.contains_key(&decl.name) || decl.important || important_map.get(&decl.name)==Some(&false);
                if should { set(&mut computed, &mut order, &decl.name, Computed::Static(decl.value.clone())); important_map.insert(decl.name.clone(), decl.important); }
            }
        }
    }
    if let Some(style) = node.get("style") {
        for decl in parse_style_declarations(style) {
            if matches!(computed.get(&decl.name), Some(Computed::Dynamic)) { continue; }
            let should = !computed.contains_key(&decl.name) || decl.important || important_map.get(&decl.name)==Some(&false);
            if should { set(&mut computed, &mut order, &decl.name, Computed::Static(decl.value.clone())); important_map.insert(decl.name.clone(), decl.important); }
        }
    }
    let _ = order;
    computed
}
pub fn compute_style(stylesheet: &Stylesheet, node: &Element, ancestors: &[&Element]) -> HashMap<String, Computed> {
    let mut computed = compute_own(stylesheet, node, ancestors);
    // ancestors parent-first
    for idx in (0..ancestors.len()).rev() {
        let anc = ancestors[idx];
        let anc_ancestors = &ancestors[..idx];
        let inherited = compute_own(stylesheet, anc, anc_ancestors);
        for (name, comp) in &inherited {
            if !computed.contains_key(name) && inheritable(name) && !presentation_non_inheritable_group(name) {
                computed.insert(name.clone(), comp.clone());
            }
        }
    }
    computed
}
pub fn has_visible_descendant(node: &Element) -> bool {
    // querySelector(node, '[visibility=visible]') != null : any descendant element with visibility attr == visible
    for c in &node.children {
        if let Node::Element(e)=c {
            if e.get("visibility")==Some("visible") { return true; }
            if has_visible_descendant(e) { return true; }
        }
    }
    false
}
