use crate::tools::{remove_leading_zero, to_fixed};

#[derive(Debug, Clone, PartialEq)]
pub struct PathItem { pub command: char, pub args: Vec<f64> }
fn args_count(c: char) -> Option<usize> {
    Some(match c { 'M'|'m'|'L'|'l'|'T'|'t'=>2, 'Z'|'z'=>0, 'H'|'h'|'V'|'v'=>1, 'C'|'c'=>6, 'S'|'s'|'Q'|'q'=>4, 'A'|'a'=>7, _=>return None })
}
fn is_ws(c: char) -> bool { matches!(c, ' '|'\t'|'\r'|'\n') }
fn read_number(chars: &[char], cursor: usize) -> (usize, Option<f64>) {
    let mut i = cursor; let mut value = String::new(); let mut state = "none";
    while i < chars.len() {
        let c = chars[i];
        if c=='+'||c=='-' {
            if state=="none" { state="sign"; value.push(c); i+=1; continue; }
            if state=="e" { state="exponent_sign"; value.push(c); i+=1; continue; }
        }
        if c.is_ascii_digit() {
            match state {
                "none"|"sign"|"whole" => state="whole",
                "decimal_point"|"decimal" => state="decimal",
                "e"|"exponent_sign"|"exponent" => state="exponent",
                _ => {}
            }
            value.push(c); i+=1; continue;
        }
        if c=='.' && matches!(state,"none"|"sign"|"whole") { state="decimal_point"; value.push(c); i+=1; continue; }
        if (c=='E'||c=='e') && matches!(state,"whole"|"decimal_point"|"decimal") { state="e"; value.push(c); i+=1; continue; }
        break;
    }
    match value.parse::<f64>() { Ok(n) => (if i==0 {0} else {i-1}, Some(n)), Err(_) => (cursor, None) }
}
pub fn parse_path_data(string: &str) -> Vec<PathItem> {
    let chars: Vec<char> = string.chars().collect();
    let mut path_data: Vec<PathItem> = Vec::new();
    let mut command: Option<char> = None;
    let mut args: Vec<f64> = Vec::new();
    let mut can_have_comma = false; let mut had_comma = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if is_ws(c) { i+=1; continue; }
        if can_have_comma && c==',' { if had_comma { break; } had_comma=true; i+=1; continue; }
        if args_count(c).is_some() {
            if had_comma { return path_data; }
            if command.is_none() { if c!='M' && c!='m' { return path_data; } }
            else if !args.is_empty() { return path_data; }
            command = Some(c); args = Vec::new();
            let ac = args_count(c).unwrap(); can_have_comma=false;
            if ac==0 { path_data.push(PathItem{command:c, args: vec![]}); }
            i+=1; continue;
        }
        let Some(cmd) = command else { return path_data };
        let ac = args_count(cmd).unwrap();
        let (new_cursor, number) = if cmd=='A'||cmd=='a' {
            let pos = args.len();
            if matches!(pos,0|1|2|5|6) { read_number(&chars, i) }
            else if pos==3||pos==4 { if c=='0' {(i, Some(0.0))} else if c=='1' {(i, Some(1.0))} else {(i, None)} }
            else { read_number(&chars, i) }
        } else { read_number(&chars, i) };
        let Some(number) = number else { return path_data };
        args.push(number); can_have_comma=true; had_comma=false; i=new_cursor;
        if args.len()==ac {
            let mut a = args.clone();
            if cmd=='A'||cmd=='a' { a[0]=a[0].abs(); a[1]=a[1].abs(); }
            path_data.push(PathItem{command:cmd, args:a});
            if command==Some('M') { command=Some('L'); }
            if command==Some('m') { command=Some('l'); }
            args = Vec::new();
        }
        i+=1;
    }
    path_data
}
fn round_and_stringify(number: f64, precision: Option<i32>) -> (String, f64) {
    let n = if let Some(p)=precision { to_fixed(number, p) } else { number };
    (remove_leading_zero(n), n)
}
fn stringify_args(command: char, args: &[f64], precision: Option<i32>, disable_space_after_flags: bool) -> String {
    let mut result = String::new(); let mut previous: Option<f64> = None;
    for (i, &arg) in args.iter().enumerate() {
        let (rounded_str, rounded) = round_and_stringify(arg, precision);
        if (disable_space_after_flags && (command=='A'||command=='a') && (i%7==4 || i%7==5))
            || i==0 || rounded < 0.0
            || (previous.is_some_and(|p| p.fract()!=0.0) && !rounded_str.chars().next().is_some_and(|c| c.is_ascii_digit())) {
            result.push_str(&rounded_str);
        } else { result.push(' '); result.push_str(&rounded_str); }
        previous = Some(rounded);
    }
    result
}
pub fn stringify_path_data(path_data: &[PathItem], precision: Option<i32>) -> String {
    stringify_path_data_opts(path_data, precision, false)
}
pub fn stringify_path_data_opts(path_data: &[PathItem], precision: Option<i32>, disable_space_after_flags: bool) -> String {
    if path_data.is_empty() { return String::new(); }
    if path_data.len()==1 { let it=&path_data[0]; return format!("{}{}", it.command, stringify_args(it.command, &it.args, precision, disable_space_after_flags)); }
    let mut result = String::new();
    let mut prev = path_data[0].clone();
    if path_data[1].command=='L' { prev.command='M'; } else if path_data[1].command=='l' { prev.command='m'; }
    for i in 1..path_data.len() {
        let cur = &path_data[i];
        let combine = (prev.command==cur.command && prev.command!='M' && prev.command!='m') || (prev.command=='M'&&cur.command=='L') || (prev.command=='m'&&cur.command=='l');
        if combine {
            prev.args.extend(cur.args.iter().cloned());
            if i==path_data.len()-1 { result.push(prev.command); result.push_str(&stringify_args(prev.command, &prev.args, precision, disable_space_after_flags)); }
        } else {
            result.push(prev.command); result.push_str(&stringify_args(prev.command, &prev.args, precision, disable_space_after_flags));
            if i==path_data.len()-1 { result.push(cur.command); result.push_str(&stringify_args(cur.command, &cur.args, precision, disable_space_after_flags)); }
            else { prev = cur.clone(); }
        }
    }
    result
}
