use crate::tools::{cleanup_out_data, to_fixed, CleanupParams};

#[derive(Clone, Debug, PartialEq)]
pub struct TransformItem {
    pub name: String,
    pub data: Vec<f64>,
}

#[derive(Clone, Debug)]
pub struct TransformParams {
    pub convert_to_shorts: bool,
    pub deg_precision: Option<i32>,
    pub float_precision: i32,
    pub transform_precision: i32,
    pub matrix_to_transform: bool,
    pub short_translate: bool,
    pub short_scale: bool,
    pub short_rotate: bool,
    pub remove_useless: bool,
    pub collapse_into_one: bool,
    pub leading_zero: bool,
    pub negative_extra_space: bool,
}

impl Default for TransformParams {
    fn default() -> Self {
        TransformParams {
            convert_to_shorts: true,
            deg_precision: None,
            float_precision: 3,
            transform_precision: 5,
            matrix_to_transform: true,
            short_translate: true,
            short_scale: true,
            short_rotate: true,
            remove_useless: true,
            collapse_into_one: true,
            leading_zero: true,
            negative_extra_space: false,
        }
    }
}

const TYPES: &[&str] = &["matrix", "rotate", "scale", "skewX", "skewY", "translate"];

/// Port of `transform2js`.
pub fn transform2js(s: &str) -> Vec<TransformItem> {
    let mut transforms: Vec<TransformItem> = Vec::new();
    // manual scan for name(...)
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // try match a transform name at i (preceded by start/ws/comma)
        let mut matched: Option<&str> = None;
        for t in TYPES {
            let tc: Vec<char> = t.chars().collect();
            if i + tc.len() <= chars.len() && chars[i..i + tc.len()] == tc[..] {
                matched = Some(t);
                break;
            }
        }
        if let Some(name) = matched {
            let mut j = i + name.len();
            while j < chars.len() && chars[j].is_whitespace() { j += 1; }
            if j < chars.len() && chars[j] == '(' {
                j += 1;
                let start = j;
                while j < chars.len() && chars[j] != ')' { j += 1; }
                if j >= chars.len() {
                    // malformed (no closing paren): JS split regex does not match it
                    i += 1;
                    continue;
                }
                let inner: String = chars[start..j].iter().collect();
                let data = parse_numbers(&inner);
                transforms.push(TransformItem { name: name.to_string(), data });
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    if transforms.is_empty() || transforms.last().is_none_or(|t| t.data.is_empty()) {
        // JS: return [] if last currentTransform null or empty data
        if transforms.iter().all(|t| t.data.is_empty()) {
            return Vec::new();
        }
        // JS checks only currentTransform (last); if empty -> []
        if transforms.last().is_some_and(|t| t.data.is_empty()) {
            return Vec::new();
        }
    }
    transforms
}

pub fn parse_numbers(s: &str) -> Vec<f64> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' {
            let start = i;
            let mut j = i;
            if j < chars.len() && (chars[j] == '+' || chars[j] == '-') { j += 1; }
            let mut seen_digit = false;
            let mut seen_dot = false;
            while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') {
                if chars[j] == '.' {
                    if seen_dot { break; }
                    seen_dot = true;
                } else {
                    seen_digit = true;
                }
                j += 1;
            }
            if j < chars.len() && (chars[j] == 'e' || chars[j] == 'E') && seen_digit {
                let mut k = j + 1;
                if k < chars.len() && (chars[k] == '+' || chars[k] == '-') { k += 1; }
                if k < chars.len() && chars[k].is_ascii_digit() {
                    while k < chars.len() && chars[k].is_ascii_digit() { k += 1; }
                    j = k;
                }
            }
            if seen_digit {
                let txt: String = chars[start..j].iter().collect();
                if let Ok(n) = txt.parse::<f64>() { out.push(n); }
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

fn rad(deg: f64) -> f64 { deg * std::f64::consts::PI / 180.0 }
fn deg(r: f64) -> f64 { r * 180.0 / std::f64::consts::PI }

pub fn transform_to_matrix(t: &TransformItem) -> Vec<f64> {
    match t.name.as_str() {
        "matrix" => t.data.clone(),
        "translate" => vec![1.0, 0.0, 0.0, 1.0, t.data[0], *t.data.get(1).unwrap_or(&0.0)],
        "scale" => vec![t.data[0], 0.0, 0.0, *t.data.get(1).unwrap_or(&t.data[0]), 0.0, 0.0],
        "rotate" => {
            let cos = rad(t.data[0]).cos();
            let sin = rad(t.data[0]).sin();
            let cx = *t.data.get(1).unwrap_or(&0.0);
            let cy = *t.data.get(2).unwrap_or(&0.0);
            vec![cos, sin, -sin, cos, (1.0 - cos) * cx + sin * cy, (1.0 - cos) * cy - sin * cx]
        }
        "skewX" => vec![1.0, 0.0, rad(t.data[0]).tan(), 1.0, 0.0, 0.0],
        "skewY" => vec![1.0, rad(t.data[0]).tan(), 0.0, 1.0, 0.0, 0.0],
        _ => vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    }
}

pub fn multiply_matrices(a: &[f64], b: &[f64]) -> Vec<f64> {
    vec![
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

/// Port of `transformsMultiply`.
pub fn transforms_multiply(transforms: &[TransformItem]) -> TransformItem {
    let matrices: Vec<Vec<f64>> = transforms.iter().map(transform_to_matrix).collect();
    let data = if matrices.is_empty() {
        Vec::new()
    } else {
        matrices.into_iter().reduce(|acc, m| multiply_matrices(&acc, &m)).unwrap()
    };
    TransformItem { name: "matrix".to_string(), data }
}

fn smart_round(precision: i32, data: &mut [f64]) {
    let tolerance = to_fixed(10f64.powi(-precision), precision);
    for i in (0..data.len()).rev() {
        if to_fixed(data[i], precision) != data[i] {
            let rounded = to_fixed(data[i], precision - 1);
            data[i] = if to_fixed((rounded - data[i]).abs(), precision + 1) >= tolerance {
                to_fixed(data[i], precision)
            } else {
                rounded
            };
        }
    }
}

fn round_int(data: &mut [f64]) {
    for v in data.iter_mut() { *v = crate::tools::js_round(*v); }
}

fn deg_round(data: &mut [f64], params: &TransformParams) {
    if let Some(dp) = params.deg_precision {
        if dp >= 1 && params.float_precision < 20 { smart_round(dp, data); return; }
    }
    round_int(data);
}
fn float_round(data: &mut [f64], params: &TransformParams) {
    if params.float_precision >= 1 && params.float_precision < 20 {
        smart_round(params.float_precision, data);
    } else {
        round_int(data);
    }
}
fn transform_round(data: &mut [f64], params: &TransformParams) {
    if params.transform_precision >= 1 && params.float_precision < 20 {
        smart_round(params.transform_precision, data);
    } else {
        round_int(data);
    }
}

/// Port of `roundTransform`.
pub fn round_transform(t: &mut TransformItem, params: &TransformParams) {
    match t.name.as_str() {
        "translate" => float_round(&mut t.data, params),
        "rotate" => {
            if !t.data.is_empty() {
                let mut head = t.data[..1].to_vec();
                deg_round(&mut head, params);
                let mut tail = t.data[1..].to_vec();
                float_round(&mut tail, params);
                head.extend(tail);
                t.data = head;
            }
        }
        "skewX" | "skewY" => deg_round(&mut t.data, params),
        "scale" => transform_round(&mut t.data, params),
        "matrix" if t.data.len() >= 4 => {
            let mut a = t.data[..4].to_vec();
            transform_round(&mut a, params);
            let mut b = t.data[4..].to_vec();
            float_round(&mut b, params);
            a.extend(b);
            t.data = a;
        }
        _ => {}
    }
}

fn decompose_qrab(matrix: &TransformItem) -> Option<Vec<TransformItem>> {
    let d = &matrix.data;
    if d.len() < 6 { return None; }
    let (a, b, c, dd, e, f) = (d[0], d[1], d[2], d[3], d[4], d[5]);
    let delta = a * dd - b * c;
    if delta == 0.0 { return None; }
    let r = (a * a + b * b).sqrt();
    if r == 0.0 { return None; }
    let mut decomp = Vec::new();
    let cos_rot = a / r;
    if e != 0.0 || f != 0.0 {
        decomp.push(TransformItem { name: "translate".into(), data: vec![e, f] });
    }
    if cos_rot != 1.0 {
        let angle = cos_rot.acos();
        decomp.push(TransformItem {
            name: "rotate".into(),
            data: vec![deg(if b < 0.0 { -angle } else { angle }), 0.0, 0.0],
        });
    }
    let sx = r;
    let sy = delta / sx;
    if sx != 1.0 || sy != 1.0 {
        decomp.push(TransformItem { name: "scale".into(), data: vec![sx, sy] });
    }
    let ac_bd = a * c + b * dd;
    if ac_bd != 0.0 {
        decomp.push(TransformItem {
            name: "skewX".into(),
            data: vec![deg((ac_bd / (a * a + b * b)).atan())],
        });
    }
    Some(decomp)
}

fn decompose_qrcd(matrix: &TransformItem) -> Option<Vec<TransformItem>> {
    let d = &matrix.data;
    if d.len() < 6 { return None; }
    let (a, b, c, dd, e, f) = (d[0], d[1], d[2], d[3], d[4], d[5]);
    let delta = a * dd - b * c;
    if delta == 0.0 { return None; }
    let s = (c * c + dd * dd).sqrt();
    if s == 0.0 { return None; }
    let mut decomp = Vec::new();
    if e != 0.0 || f != 0.0 {
        decomp.push(TransformItem { name: "translate".into(), data: vec![e, f] });
    }
    let angle = std::f64::consts::PI / 2.0 - (if dd < 0.0 { -1.0 } else { 1.0 }) * (-c / s).acos();
    decomp.push(TransformItem { name: "rotate".into(), data: vec![deg(angle), 0.0, 0.0] });
    let sx = delta / s;
    let sy = s;
    if sx != 1.0 || sy != 1.0 {
        decomp.push(TransformItem { name: "scale".into(), data: vec![sx, sy] });
    }
    let ac_bd = a * c + b * dd;
    if ac_bd != 0.0 {
        decomp.push(TransformItem {
            name: "skewY".into(),
            data: vec![deg((ac_bd / (c * c + dd * dd)).atan())],
        });
    }
    Some(decomp)
}

fn merge_translate_and_rotate(tx: f64, ty: f64, a: f64) -> TransformItem {
    let r = rad(a);
    let d = 1.0 - r.cos();
    let e = r.sin();
    let cy = (d * ty + e * tx) / (d * d + e * e);
    let cx = (tx - e * cy) / d;
    TransformItem { name: "rotate".into(), data: vec![a, cx, cy] }
}

fn is_identity(t: &TransformItem) -> bool {
    match t.name.as_str() {
        "rotate" | "skewX" | "skewY" => t.data.first().is_some_and(|v| *v == 0.0),
        "scale" => t.data.first().is_some_and(|v| *v == 1.0)
            && t.data.get(1).is_none_or(|v| *v == 1.0),
        "translate" => t.data.first().is_some_and(|v| *v == 0.0)
            && t.data.get(1).is_none_or(|v| *v == 0.0),
        _ => false,
    }
}

fn create_scale(data: &[f64]) -> TransformItem {
    let n = if data.len() >= 2 && data[0] == data[1] { 1 } else { data.len().min(2) };
    TransformItem { name: "scale".into(), data: data[..n.max(1)].to_vec() }
}

fn optimize_transforms(rounded: &[TransformItem], raw: &[TransformItem]) -> Vec<TransformItem> {
    let mut out: Vec<TransformItem> = Vec::new();
    let mut index = 0;
    while index < rounded.len() {
        let rt = &rounded[index];
        if is_identity(rt) { index += 1; continue; }
        let data = &rt.data;
        match rt.name.as_str() {
            "rotate" => {
                if data[0] == 180.0 || data[0] == -180.0 {
                    if let Some(next) = rounded.get(index + 1) {
                        if next.name == "scale" {
                            out.push(create_scale(&next.data.iter().map(|v| -v).collect::<Vec<_>>()));
                            index += 2;
                            continue;
                        }
                    }
                    out.push(TransformItem { name: "scale".into(), data: vec![-1.0] });
                    index += 1;
                    continue;
                }
                let n = if data.get(1).is_some_and(|v| *v != 0.0)
                    || data.get(2).is_some_and(|v| *v != 0.0) { 3 } else { 1 };
                out.push(TransformItem { name: "rotate".into(), data: data[..n.min(data.len())].to_vec() });
            }
            "scale" => out.push(create_scale(data)),
            "skewX" | "skewY" => out.push(TransformItem { name: rt.name.clone(), data: vec![data[0]] }),
            "translate" => {
                if let Some(next) = rounded.get(index + 1) {
                    if next.name == "rotate"
                        && next.data[0] != 180.0 && next.data[0] != -180.0 && next.data[0] != 0.0
                        && next.data.get(1).is_none_or(|v| *v == 0.0)
                        && next.data.get(2).is_none_or(|v| *v == 0.0)
                    {
                        let raw_data = &raw[index].data;
                        out.push(merge_translate_and_rotate(
                            raw_data[0],
                            *raw_data.get(1).unwrap_or(&0.0),
                            raw[index + 1].data[0],
                        ));
                        index += 2;
                        continue;
                    }
                }
                let n = if data.get(1).is_some_and(|v| *v != 0.0) { 2 } else { 1 };
                out.push(TransformItem { name: "translate".into(), data: data[..n.min(data.len())].to_vec() });
            }
            _ => out.push(rt.clone()),
        }
        index += 1;
    }
    if out.is_empty() {
        vec![TransformItem { name: "scale".into(), data: vec![1.0] }]
    } else {
        out
    }
}

/// Port of `matrixToTransform`.
pub fn matrix_to_transform(orig: &TransformItem, params: &TransformParams) -> Vec<TransformItem> {
    let mut decomps = Vec::new();
    if let Some(d) = decompose_qrab(orig) { decomps.push(d); }
    if let Some(d) = decompose_qrcd(orig) { decomps.push(d); }
    let mut shortest: Option<Vec<TransformItem>> = None;
    let mut shortest_len = usize::MAX;
    for decomp in &decomps {
        let rounded: Vec<TransformItem> = decomp.iter().map(|t| {
            let mut c = t.clone();
            round_transform(&mut c, params);
            c
        }).collect();
        let optimized = optimize_transforms(&rounded, decomp);
        let len = js2transform(&optimized, params).len();
        if len < shortest_len {
            shortest = Some(optimized);
            shortest_len = len;
        }
    }
    shortest.unwrap_or_else(|| vec![orig.clone()])
}

/// Port of `transformArc` (_transforms.js).
pub fn transform_arc(cursor: [f64; 2], arc: &mut [f64], transform: &[f64]) {
    let x = arc[5] - cursor[0];
    let y = arc[6] - cursor[1];
    let mut a = arc[0];
    let mut b = arc[1];
    let rot = arc[2] * std::f64::consts::PI / 180.0;
    let cos = rot.cos();
    let sin = rot.sin();
    if a > 0.0 && b > 0.0 {
        let mut h = (x * cos + y * sin).powi(2) / (4.0 * a * a)
            + (y * cos - x * sin).powi(2) / (4.0 * b * b);
        if h > 1.0 {
            h = h.sqrt();
            a *= h;
            b *= h;
        }
    }
    let ellipse = vec![a * cos, a * sin, -b * sin, b * cos, 0.0, 0.0];
    let m = multiply_matrices(transform, &ellipse);
    let last_col = m[2] * m[2] + m[3] * m[3];
    let square_sum = m[0] * m[0] + m[1] * m[1] + last_col;
    let root = (m[0] - m[3]).hypot(m[1] + m[2]) * (m[0] + m[3]).hypot(m[1] - m[2]);
    if root == 0.0 {
        arc[0] = (square_sum / 2.0).sqrt();
        arc[1] = arc[0];
        arc[2] = 0.0;
    } else {
        let major_sqr = (square_sum + root) / 2.0;
        let minor_sqr = (square_sum - root) / 2.0;
        let major = (major_sqr - last_col).abs() > 1e-6;
        let sub = (if major { major_sqr } else { minor_sqr }) - last_col;
        let rows_sum = m[0] * m[2] + m[1] * m[3];
        let term1 = m[0] * sub + m[2] * rows_sum;
        let term2 = m[1] * sub + m[3] * rows_sum;
        arc[0] = major_sqr.sqrt();
        arc[1] = minor_sqr.sqrt();
        arc[2] = ((if (major && term2 < 0.0) || (!major && term1 > 0.0) { -1.0 } else { 1.0 })
            * (if major { term1 } else { term2 } / term1.hypot(term2)).acos()
            * 180.0)
            / std::f64::consts::PI;
    }
    if (transform[0] < 0.0) != (transform[3] < 0.0) {
        arc[4] = 1.0 - arc[4];
    }
}

/// Port of `js2transform`.
pub fn js2transform(transforms: &[TransformItem], params: &TransformParams) -> String {
    let mut out = String::new();
    for t in transforms {
        let mut copy = t.clone();
        round_transform(&mut copy, params);
        let cp = CleanupParams {
            leading_zero: params.leading_zero,
            negative_extra_space: params.negative_extra_space,
            no_space_after_flags: false,
        };
        out.push_str(&format!("{}({})", copy.name, cleanup_out_data(&copy.data, &cp, None)));
    }
    out
}
