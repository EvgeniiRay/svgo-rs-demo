use crate::tools::{remove_leading_zero, to_fixed};

#[derive(Clone, Debug, PartialEq)]
pub struct PathItem {
    pub command: char,
    pub args: Vec<f64>,
    pub base: [f64; 2],
    pub coords: [f64; 2],
    pub sdata: Option<Vec<f64>>,
}

impl PathItem {
    pub fn new(command: char, args: Vec<f64>) -> Self {
        PathItem { command, args, base: [0.0, 0.0], coords: [0.0, 0.0], sdata: None }
    }
}

pub fn args_count(c: char) -> Option<usize> {
    match c {
        'M' | 'm' | 'L' | 'l' | 'T' | 't' => Some(2),
        'Z' | 'z' => Some(0),
        'H' | 'h' | 'V' | 'v' => Some(1),
        'C' | 'c' => Some(6),
        'S' | 's' | 'Q' | 'q' => Some(4),
        'A' | 'a' => Some(7),
        _ => None,
    }
}

fn is_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn read_number(chars: &[char], cursor: usize) -> (usize, Option<f64>) {
    let mut i = cursor;
    let mut value = String::new();
    #[derive(PartialEq)]
    enum St { None, Sign, Whole, DecPoint, Dec, E, ExpSign, Exp }
    let mut state = St::None;
    while i < chars.len() {
        let c = chars[i];
        if c == '+' || c == '-' {
            if state == St::None { state = St::Sign; value.push(c); i += 1; continue; }
            if state == St::E { state = St::ExpSign; value.push(c); i += 1; continue; }
        }
        if c.is_ascii_digit() {
            match state {
                St::None | St::Sign | St::Whole => { state = St::Whole; value.push(c); i += 1; continue; }
                St::DecPoint | St::Dec => { state = St::Dec; value.push(c); i += 1; continue; }
                St::E | St::ExpSign | St::Exp => { state = St::Exp; value.push(c); i += 1; continue; }
            }
        }
        if c == '.' && matches!(state, St::None | St::Sign | St::Whole) {
            state = St::DecPoint; value.push(c); i += 1; continue;
        }
        if (c == 'E' || c == 'e') && matches!(state, St::Whole | St::DecPoint | St::Dec) {
            state = St::E; value.push(c); i += 1; continue;
        }
        break;
    }
    match value.parse::<f64>() {
        Ok(n) if !value.is_empty() && value != "+" && value != "-" && value != "." => {
            (if i == 0 { 0 } else { i - 1 }, Some(n))
        }
        _ => (cursor, None),
    }
}

/// Port of `parsePathData` (lib/path.js).
pub fn parse_path_data(string: &str) -> Vec<PathItem> {
    let chars: Vec<char> = string.chars().collect();
    let mut path_data = Vec::new();
    let mut command: Option<char> = None;
    let mut args: Vec<f64> = Vec::new();
    let mut args_len = 0usize;
    let mut can_have_comma = false;
    let mut had_comma = false;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if is_ws(c) { i += 1; continue; }
        if can_have_comma && c == ',' {
            if had_comma { break; }
            had_comma = true;
            i += 1;
            continue;
        }
        if args_count(c).is_some() {
            if had_comma { return path_data; }
            if command.is_none() {
                if c != 'M' && c != 'm' { return path_data; }
            } else if !args.is_empty() {
                return path_data;
            }
            command = Some(c);
            args = Vec::new();
            args_len = args_count(c).unwrap();
            can_have_comma = false;
            if args_len == 0 {
                path_data.push(PathItem::new(c, Vec::new()));
            }
            i += 1;
            continue;
        }
        if command.is_none() { return path_data; }
        let cmd = command.unwrap();
        let (new_cursor, number) = if cmd == 'A' || cmd == 'a' {
            let position = args.len();
            if position == 3 || position == 4 {
                if c == '0' { (i, Some(0.0)) } else if c == '1' { (i, Some(1.0)) } else { (i, None) }
            } else {
                read_number(&chars, i)
            }
        } else {
            read_number(&chars, i)
        };
        let number = match number {
            Some(n) => n,
            None => return path_data,
        };
        args.push(number);
        can_have_comma = true;
        had_comma = false;
        i = new_cursor + 1;
        if args.len() == args_len {
            if cmd == 'A' || cmd == 'a' {
                args[0] = args[0].abs();
                args[1] = args[1].abs();
            }
            let mut pushed_cmd = cmd;
            path_data.push(PathItem::new(pushed_cmd, std::mem::take(&mut args)));
            if pushed_cmd == 'M' { pushed_cmd = 'L'; command = Some('L'); }
            if pushed_cmd == 'm' { pushed_cmd = 'l'; command = Some('l'); }
            let _ = pushed_cmd;
        }
    }
    path_data
}

/// Port of `path2js` first-moveto fixup.
pub fn path2js(d: &str) -> Vec<PathItem> {
    let mut data = parse_path_data(d);
    if !data.is_empty() && data[0].command == 'm' {
        data[0].command = 'M';
    }
    data
}

fn round_and_stringify(number: f64, precision: Option<i32>) -> (String, f64) {
    let n = match precision {
        Some(p) => to_fixed(number, p),
        None => number,
    };
    (remove_leading_zero(n), n)
}

fn stringify_args(command: char, args: &[f64], precision: Option<i32>, disable_space_after_flags: bool) -> String {
    let mut result = String::new();
    let mut previous: Option<f64> = None;
    for (i, arg) in args.iter().enumerate() {
        let (rounded_str, rounded) = round_and_stringify(*arg, precision);
        let first_is_digit = rounded_str.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false);
        let prev_not_int = previous.map(|p| p != p.trunc()).unwrap_or(false);
        if disable_space_after_flags && (command == 'A' || command == 'a') && (i % 7 == 4 || i % 7 == 5) {
            result.push_str(&rounded_str);
        } else if i == 0 || rounded < 0.0 {
            result.push_str(&rounded_str);
        } else if prev_not_int && !first_is_digit {
            result.push_str(&rounded_str);
        } else {
            result.push(' ');
            result.push_str(&rounded_str);
        }
        previous = Some(rounded);
    }
    result
}

/// Port of `stringifyPathData` (lib/path.js).
pub fn stringify_path_data(path_data: &[PathItem], precision: Option<i32>, disable_space_after_flags: bool) -> String {
    if path_data.is_empty() {
        return String::new();
    }
    if path_data.len() == 1 {
        let item = &path_data[0];
        return format!("{}{}", item.command, stringify_args(item.command, &item.args, precision, disable_space_after_flags));
    }
    let mut result = String::new();
    let mut prev = path_data[0].clone();
    if path_data[1].command == 'L' {
        prev.command = 'M';
    } else if path_data[1].command == 'l' {
        prev.command = 'm';
    }
    for i in 1..path_data.len() {
        let item = &path_data[i];
        let (command, ref args) = (item.command, &item.args);
        if (prev.command == command && prev.command != 'M' && prev.command != 'm')
            || (prev.command == 'M' && command == 'L')
            || (prev.command == 'm' && command == 'l')
        {
            prev.args.extend(args.iter().cloned());
            if i == path_data.len() - 1 {
                result.push_str(&format!("{}{}", prev.command, stringify_args(prev.command, &prev.args, precision, disable_space_after_flags)));
            }
        } else {
            result.push_str(&format!("{}{}", prev.command, stringify_args(prev.command, &prev.args, precision, disable_space_after_flags)));
            if i == path_data.len() - 1 {
                result.push_str(&format!("{}{}", command, stringify_args(command, args, precision, disable_space_after_flags)));
            } else {
                prev = item.clone();
            }
        }
    }
    result
}

/// Port of `convertRelativeToAbsolute` (plugins/_path.js).
pub fn convert_relative_to_absolute(data: &[PathItem]) -> Vec<PathItem> {
    let mut new_data = Vec::new();
    let mut start = [0.0f64; 2];
    let mut cursor = [0.0f64; 2];
    for item in data {
        let mut command = item.command;
        let mut args = item.args.clone();
        match command {
            'm' => { args[0] += cursor[0]; args[1] += cursor[1]; command = 'M'; }
            _ => {}
        }
        if command == 'M' { cursor = [args[0], args[1]]; start = cursor; }
        if command == 'h' { args[0] += cursor[0]; command = 'H'; }
        if command == 'H' { cursor[0] = args[0]; }
        if command == 'v' { args[0] += cursor[1]; command = 'V'; }
        if command == 'V' { cursor[1] = args[0]; }
        if command == 'l' { args[0] += cursor[0]; args[1] += cursor[1]; command = 'L'; }
        if command == 'L' { cursor = [args[0], args[1]]; }
        if command == 'c' { for k in 0..6 { args[k] += cursor[k % 2]; } command = 'C'; }
        if command == 'C' { cursor = [args[4], args[5]]; }
        if command == 's' { for k in 0..4 { args[k] += cursor[k % 2]; } command = 'S'; }
        if command == 'S' { cursor = [args[2], args[3]]; }
        if command == 'q' { for k in 0..4 { args[k] += cursor[k % 2]; } command = 'Q'; }
        if command == 'Q' { cursor = [args[2], args[3]]; }
        if command == 't' { args[0] += cursor[0]; args[1] += cursor[1]; command = 'T'; }
        if command == 'T' { cursor = [args[0], args[1]]; }
        if command == 'a' { args[5] += cursor[0]; args[6] += cursor[1]; command = 'A'; }
        if command == 'A' { cursor = [args[5], args[6]]; }
        if command == 'z' || command == 'Z' { cursor = start; command = 'z'; }
        new_data.push(PathItem::new(command, args));
    }
    new_data
}

type Pt = [f64; 2];

#[derive(Clone, Debug)]
struct PointSet {
    list: Vec<Pt>,
    min_x: usize,
    min_y: usize,
    max_x: usize,
    max_y: usize,
}

impl PointSet {
    fn new() -> Self {
        PointSet { list: Vec::new(), min_x: 0, min_y: 0, max_x: 0, max_y: 0 }
    }
}

#[derive(Clone, Debug)]
struct Points {
    list: Vec<PointSet>,
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

fn gather_points(path_data: &[PathItem]) -> Points {
    let mut points = Points { list: Vec::new(), min_x: 0.0, min_y: 0.0, max_x: 0.0, max_y: 0.0 };
    let mut prev_ctrl_point: Option<Pt> = None;

    fn add_point(points: &mut Points, sub_idx: usize, point: Pt) {
        let (set_len, cur_max_y, cur_max_x, cur_min_y, cur_min_x) = {
            let set = &points.list[sub_idx];
            (
                set.list.len(),
                set.list.get(set.max_y).map(|p| p[1]),
                set.list.get(set.max_x).map(|p| p[0]),
                set.list.get(set.min_y).map(|p| p[1]),
                set.list.get(set.min_x).map(|p| p[0]),
            )
        };
        let n_sets = points.list.len();
        if set_len == 0 || point[1] > cur_max_y.unwrap() {
            points.list[sub_idx].max_y = set_len;
            points.max_y = if n_sets > 0 { points.max_y.max(point[1]) } else { point[1] };
            // JS: points.list.length ? max : point — list is non-empty once a subpath exists;
            // for the very first point subpath was just pushed, so n_sets>=1 there.
            if n_sets == 0 {
                points.max_y = point[1];
            }
        }
        if set_len == 0 || point[0] > cur_max_x.unwrap() {
            points.list[sub_idx].max_x = set_len;
            points.max_x = if n_sets > 0 { points.max_x.max(point[0]) } else { point[0] };
        }
        if set_len == 0 || point[1] < cur_min_y.unwrap() {
            points.list[sub_idx].min_y = set_len;
            points.min_y = if n_sets > 0 { points.min_y.min(point[1]) } else { point[1] };
        }
        if set_len == 0 || point[0] < cur_min_x.unwrap() {
            points.list[sub_idx].min_x = set_len;
            points.min_x = if n_sets > 0 { points.min_x.min(point[0]) } else { point[0] };
        }
        points.list[sub_idx].list.push(point);
    }

    for i in 0..path_data.len() {
        let item = &path_data[i];
        let mut sub_idx: usize;
        if points.list.is_empty() {
            points.list.push(PointSet::new());
            sub_idx = 0;
        } else {
            sub_idx = points.list.len() - 1;
        }
        if item.command == 'M' {
            points.list.push(PointSet::new());
            sub_idx = points.list.len() - 1;
        }
        let prev_cmd = if i == 0 { None } else { Some(path_data[i - 1].command) };
        let base_point: Option<Pt> = points.list[sub_idx].list.last().cloned();
        let data = &item.args;
        let mut ctrl_point: Option<Pt> = base_point;

        match item.command {
            'H' => {
                if let Some(b) = base_point {
                    add_point(&mut points, sub_idx, [data[0], b[1]]);
                }
            }
            'V' => {
                if let Some(b) = base_point {
                    add_point(&mut points, sub_idx, [b[0], data[0]]);
                }
            }
            'Q' => {
                add_point(&mut points, sub_idx, [data[0], data[1]]);
                prev_ctrl_point = Some([data[2] - data[0], data[3] - data[1]]);
            }
            'T' => {
                if let (Some(b), Some(pc)) = (base_point, prev_ctrl_point) {
                    if prev_cmd == Some('Q') || prev_cmd == Some('T') {
                        let ctrl = [b[0] + pc[0], b[1] + pc[1]];
                        add_point(&mut points, sub_idx, ctrl);
                        prev_ctrl_point = Some([data[0] - ctrl[0], data[1] - ctrl[1]]);
                    }
                }
            }
            'C' => {
                if let Some(b) = base_point {
                    add_point(&mut points, sub_idx, [0.5 * (b[0] + data[0]), 0.5 * (b[1] + data[1])]);
                }
                add_point(&mut points, sub_idx, [0.5 * (data[0] + data[2]), 0.5 * (data[1] + data[3])]);
                add_point(&mut points, sub_idx, [0.5 * (data[2] + data[4]), 0.5 * (data[3] + data[5])]);
                prev_ctrl_point = Some([data[4] - data[2], data[5] - data[3]]);
            }
            'S' => {
                if let (Some(b), Some(pc)) = (base_point, prev_ctrl_point) {
                    if prev_cmd == Some('C') || prev_cmd == Some('S') {
                        add_point(&mut points, sub_idx, [b[0] + 0.5 * pc[0], b[1] + 0.5 * pc[1]]);
                        ctrl_point = Some([b[0] + pc[0], b[1] + pc[1]]);
                    }
                }
                if let Some(cp) = ctrl_point {
                    // JS adds this only `if (ctrlPoint != null)`; ctrlPoint defaults to basePoint
                    if base_point.is_some() || ctrl_point.is_some() {
                        add_point(&mut points, sub_idx, [0.5 * (cp[0] + data[0]), 0.5 * (cp[1] + data[1])]);
                    }
                }
                add_point(&mut points, sub_idx, [0.5 * (data[0] + data[2]), 0.5 * (data[1] + data[3])]);
                prev_ctrl_point = Some([data[2] - data[0], data[3] - data[1]]);
            }
            'A' => {
                if let Some(b) = base_point {
                    let mut curves = a2c(b[0], b[1], data[0], data[1], data[2], data[3], data[4], data[5], data[6]);
                    let mut bp = b;
                    while curves.len() >= 6 {
                        let chunk: Vec<f64> = curves.drain(..6).collect();
                        let abs: Vec<f64> = chunk
                            .iter()
                            .enumerate()
                            .map(|(k, v)| v + bp[k % 2])
                            .collect();
                        add_point(&mut points, sub_idx, [0.5 * (bp[0] + abs[0]), 0.5 * (bp[1] + abs[1])]);
                        add_point(&mut points, sub_idx, [0.5 * (abs[0] + abs[2]), 0.5 * (abs[1] + abs[3])]);
                        add_point(&mut points, sub_idx, [0.5 * (abs[2] + abs[4]), 0.5 * (abs[3] + abs[5])]);
                        if !curves.is_empty() {
                            bp = [abs[4], abs[5]];
                            add_point(&mut points, sub_idx, bp);
                        }
                    }
                }
            }
            _ => {}
        }
        if data.len() >= 2 {
            add_point(&mut points, sub_idx, [data[data.len() - 2], data[data.len() - 1]]);
        }
    }
    points
}

fn cross(o: Pt, a: Pt, b: Pt) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

fn convex_hull(points: &PointSet) -> PointSet {
    let mut list = points.list.clone();
    list.sort_by(|a, b| {
        if a[0] == b[0] {
            a[1].partial_cmp(&b[1]).unwrap()
        } else {
            a[0].partial_cmp(&b[0]).unwrap()
        }
    });
    let mut lower: Vec<Pt> = Vec::new();
    let mut min_y = 0usize;
    let mut bottom = 0usize;
    for i in 0..list.len() {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], list[i]) <= 0.0 {
            lower.pop();
        }
        if list[i][1] < list[min_y][1] {
            min_y = i;
            bottom = lower.len();
        }
        lower.push(list[i]);
    }
    let mut upper: Vec<Pt> = Vec::new();
    let mut max_y = if list.is_empty() { 0 } else { list.len() - 1 };
    let mut top = 0usize;
    for i in (0..list.len()).rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], list[i]) <= 0.0 {
            upper.pop();
        }
        if list[i][1] > list[max_y][1] {
            max_y = i;
            top = upper.len();
        }
        upper.push(list[i]);
    }
    upper.pop();
    lower.pop();
    let max_x = lower.len();
    let hull_len_base = lower.len();
    let mut hull_list = lower;
    hull_list.extend(upper);
    let hull_len = hull_list.len();
    PointSet {
        list: hull_list,
        min_x: 0,
        max_x,
        min_y: bottom,
        max_y: if hull_len == 0 { 0 } else { (hull_len_base + top) % hull_len },
    }
}

fn minus(v: Pt) -> Pt {
    [-v[0], -v[1]]
}
fn sub(a: Pt, b: Pt) -> Pt {
    [a[0] - b[0], a[1] - b[1]]
}
fn dot(a: Pt, b: Pt) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn orth(v: Pt, from: Pt) -> Pt {
    let o = [-v[1], v[0]];
    if dot(o, minus(from)) < 0.0 {
        minus(o)
    } else {
        o
    }
}
fn set_dir(dest: &mut Pt, source: Pt) {
    *dest = source;
}

fn support_point(polygon: &PointSet, direction: Pt) -> Pt {
    let mut index = if direction[1] >= 0.0 {
        if direction[0] < 0.0 { polygon.max_y } else { polygon.max_x }
    } else if direction[0] < 0.0 {
        polygon.min_x
    } else {
        polygon.min_y
    };
    let mut max = f64::NEG_INFINITY;
    loop {
        let value = dot(polygon.list[index], direction);
        if !(value > max) {
            break;
        }
        max = value;
        index = (index + 1) % polygon.list.len();
    }
    let idx = if index == 0 { polygon.list.len() - 1 } else { index - 1 };
    polygon.list[idx]
}

fn get_support(a: &PointSet, b: &PointSet, direction: Pt) -> Pt {
    sub(support_point(a, direction), support_point(b, minus(direction)))
}

fn process_simplex(simplex: &mut Vec<Pt>, direction: &mut Pt) -> bool {
    if simplex.len() == 2 {
        let a = simplex[1];
        let b = simplex[0];
        let ao = minus(a);
        let ab = sub(b, a);
        if dot(ao, ab) > 0.0 {
            set_dir(direction, orth(ab, a));
        } else {
            set_dir(direction, ao);
            simplex.remove(0);
        }
    } else if simplex.len() == 3 {
        let a = simplex[2];
        let b = simplex[1];
        let c = simplex[0];
        let ab = sub(b, a);
        let ac = sub(c, a);
        let ao = minus(a);
        let acb = orth(ab, ac);
        let abc = orth(ac, ab);
        if dot(acb, ao) > 0.0 {
            if dot(ab, ao) > 0.0 {
                set_dir(direction, acb);
                simplex.remove(0);
            } else {
                set_dir(direction, ao);
                simplex.drain(..2);
            }
        } else if dot(abc, ao) > 0.0 {
            if dot(ac, ao) > 0.0 {
                set_dir(direction, abc);
                simplex.remove(1);
            } else {
                set_dir(direction, ao);
                simplex.drain(..2);
            }
        } else {
            return true;
        }
    }
    false
}

/// Port of `intersects` (plugins/_path.js): GJK over monotone-chain convex hulls.
pub fn intersects(path1: &[PathItem], path2: &[PathItem]) -> bool {
    let points1 = gather_points(&convert_relative_to_absolute(path1));
    let points2 = gather_points(&convert_relative_to_absolute(path2));

    if points1.max_x <= points2.min_x
        || points2.max_x <= points1.min_x
        || points1.max_y <= points2.min_y
        || points2.max_y <= points1.min_y
        || points1.list.iter().all(|set1| {
            points2.list.iter().all(|set2| {
                if set1.list.is_empty() || set2.list.is_empty() {
                    return true;
                }
                set1.list[set1.max_x][0] <= set2.list[set2.min_x][0]
                    || set2.list[set2.max_x][0] <= set1.list[set1.min_x][0]
                    || set1.list[set1.max_y][1] <= set2.list[set2.min_y][1]
                    || set2.list[set2.max_y][1] <= set1.list[set1.min_y][1]
            })
        })
    {
        return false;
    }

    let hulls1: Vec<PointSet> = points1.list.iter().map(convex_hull).collect();
    let hulls2: Vec<PointSet> = points2.list.iter().map(convex_hull).collect();

    for hull1 in &hulls1 {
        if hull1.list.len() < 3 {
            continue;
        }
        for hull2 in &hulls2 {
            if hull2.list.len() < 3 {
                continue;
            }
            let mut simplex: Vec<Pt> = vec![get_support(hull1, hull2, [1.0, 0.0])];
            let mut direction: Pt = minus(simplex[0]);
            let mut iterations = 10000i64;
            loop {
                if iterations == 0 {
                    return true;
                }
                iterations -= 1;
                simplex.push(get_support(hull1, hull2, direction));
                if dot(direction, *simplex.last().unwrap()) <= 0.0 {
                    // JS returns false only for this hull pair (Array.some callback)
                    break;
                }
                if process_simplex(&mut simplex, &mut direction) {
                    return true;
                }
            }
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn a2c(x1: f64, y1: f64, rx0: f64, ry0: f64, angle: f64, large_arc: f64, sweep: f64, x2: f64, y2: f64) -> Vec<f64> {
    a2c_inner(x1, y1, rx0, ry0, angle, large_arc, sweep, x2, y2, None)
}

#[allow(clippy::too_many_arguments)]
fn a2c_inner(x1i: f64, y1i: f64, rx0: f64, ry0: f64, angle: f64, large_arc: f64, sweep: f64, x2i: f64, y2i: f64, recursive: Option<[f64; 4]>) -> Vec<f64> {
    let c120 = std::f64::consts::PI * 120.0 / 180.0;
    let rad = std::f64::consts::PI / 180.0 * angle;
    let (mut rx, mut ry) = (rx0, ry0);
    let (mut x1, mut y1, mut x2, mut y2) = (x1i, y1i, x2i, y2i);
    let rotate_x = |x: f64, y: f64, r: f64| x * r.cos() - y * r.sin();
    let rotate_y = |x: f64, y: f64, r: f64| x * r.sin() + y * r.cos();
    let (f1, mut f2, cx, cy);
    if recursive.is_none() {
        let nx1 = rotate_x(x1, y1, -rad);
        y1 = rotate_y(x1, y1, -rad);
        x1 = nx1;
        let nx2 = rotate_x(x2, y2, -rad);
        y2 = rotate_y(x2, y2, -rad);
        x2 = nx2;
        let x = (x1 - x2) / 2.0;
        let y = (y1 - y2) / 2.0;
        let mut h = (x * x) / (rx * rx) + (y * y) / (ry * ry);
        if h > 1.0 { h = h.sqrt(); rx = h * rx; ry = h * ry; }
        let rx2 = rx * rx;
        let ry2 = ry * ry;
        let k = (if large_arc == sweep { -1.0 } else { 1.0 })
            * (((rx2 * ry2 - rx2 * y * y - ry2 * x * x) / (rx2 * y * y + ry2 * x * x)).abs()).sqrt();
        cx = (k * rx * y) / ry + (x1 + x2) / 2.0;
        cy = (k * -ry * x) / rx + (y1 + y2) / 2.0;
        // JS: Math.asin(Number(((y1-cy)/ry).toFixed(9)))
        let mut ff1 = (((y1 - cy) / ry * 1e9).round() / 1e9).asin();
        let mut ff2 = (((y2 - cy) / ry * 1e9).round() / 1e9).asin();
        let _ = ff1;
        if x1 < cx { ff1 = std::f64::consts::PI - ff1; }
        if x2 < cx { ff2 = std::f64::consts::PI - ff2; }
        if ff1 < 0.0 { ff1 = 2.0 * std::f64::consts::PI + ff1; }
        if ff2 < 0.0 { ff2 = 2.0 * std::f64::consts::PI + ff2; }
        if sweep != 0.0 && ff1 > ff2 { ff1 -= 2.0 * std::f64::consts::PI; }
        if sweep == 0.0 && ff2 > ff1 { ff2 -= 2.0 * std::f64::consts::PI; }
        f1 = ff1;
        f2 = ff2;
    } else {
        let r = recursive.unwrap();
        f1 = r[0]; f2 = r[1]; cx = r[2]; cy = r[3];
    }
    let mut res: Vec<f64> = Vec::new();
    let mut df = f2 - f1;
    if df.abs() > c120 {
        let f2old = f2;
        let x2old = x2;
        let y2old = y2;
        f2 = f1 + c120 * (if sweep != 0.0 && f2 > f1 { 1.0 } else { -1.0 });
        x2 = cx + rx * f2.cos();
        y2 = cy + ry * f2.sin();
        res = a2c_inner(x2, y2, rx, ry, angle, 0.0, sweep, x2old, y2old, Some([f2, f2old, cx, cy]));
    }
    df = f2 - f1;
    let c1 = f1.cos();
    let s1 = f1.sin();
    let c2 = f2.cos();
    let s2 = f2.sin();
    let t = (df / 4.0).tan();
    let hx = (4.0 / 3.0) * rx * t;
    let hy = (4.0 / 3.0) * ry * t;
    let m = vec![-hx * s1, hy * c1, x2 + hx * s2 - x1, y2 - hy * c2 - y1, x2 - x1, y2 - y1];
    if recursive.is_some() {
        let mut out = m;
        out.extend(res);
        return out;
    }
    let mut full = m;
    full.extend(res);
    let mut newres = vec![0.0; full.len()];
    for i in 0..full.len() {
        newres[i] = if i % 2 == 1 {
            rotate_y(full[i - 1], full[i], rad)
        } else {
            rotate_x(full[i], full[i + 1], rad)
        };
    }
    newres
}

/// Port of `js2path` (plugins/_path.js): serializes only, with no
/// moveto removal. Consecutive movetos are left to `collapseRepeated`
/// in convertPathData, which already merges them; when that is disabled
/// or the path has mid markers they must be kept, because a following
/// relative moveto is measured from the previous moveto's point and
/// every moveto is a marker vertex.
pub fn js2path(data: &[PathItem], float_precision: Option<i32>, no_space_after_flags: bool) -> String {
    stringify_path_data(data, float_precision, no_space_after_flags)
}
