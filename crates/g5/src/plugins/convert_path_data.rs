use crate::dom::{Document, Element, Node};
use crate::json::Json;
use crate::path::{js2path, path2js, PathItem};
use crate::style::{collect_stylesheet, compute_prop, Stylesheet};
use crate::tools::{cleanup_out_data, remove_leading_zero, to_fixed, CleanupParams};
use crate::transforms::{transform2js, transform_arc, transforms_multiply};

#[derive(Clone, Debug)]
pub struct Params {
    pub apply_transforms: bool,
    pub apply_transforms_stroked: bool,
    pub make_arcs: Option<(f64, f64)>,
    pub straight_curves: bool,
    pub convert_to_q: bool,
    pub line_shorthands: bool,
    pub convert_to_z: bool,
    pub curve_smooth_shorthands: bool,
    /// None means `false` in JS.
    pub float_precision: Option<i32>,
    pub transform_precision: i32,
    pub smart_arc_rounding: bool,
    pub remove_useless: bool,
    pub collapse_repeated: bool,
    pub utilize_absolute: bool,
    pub leading_zero: bool,
    pub negative_extra_space: bool,
    pub no_space_after_flags: bool,
    pub force_absolute_path: bool,
}

impl Params {
    fn from_json(p: &Json) -> Self {
        let make_arcs = match p.get("makeArcs") {
            Some(Json::Bool(false)) => None,
            Some(Json::Obj(_)) => {
                let obj = p.get("makeArcs").unwrap();
                Some((
                    obj.num_param("threshold", 2.5),
                    obj.num_param("tolerance", 0.5),
                ))
            }
            _ => Some((2.5, 0.5)),
        };
        let float_precision = match p.get("floatPrecision") {
            Some(Json::Bool(false)) => None,
            Some(Json::Num(n)) => Some(*n as i32),
            _ => Some(3),
        };
        Params {
            apply_transforms: p.bool_param("applyTransforms", true),
            apply_transforms_stroked: p.bool_param("applyTransformsStroked", true),
            make_arcs,
            straight_curves: p.bool_param("straightCurves", true),
            convert_to_q: p.bool_param("convertToQ", true),
            line_shorthands: p.bool_param("lineShorthands", true),
            convert_to_z: p.bool_param("convertToZ", true),
            curve_smooth_shorthands: p.bool_param("curveSmoothShorthands", true),
            float_precision,
            transform_precision: p.num_param("transformPrecision", 5.0) as i32,
            smart_arc_rounding: p.bool_param("smartArcRounding", true),
            remove_useless: p.bool_param("removeUseless", true),
            collapse_repeated: p.bool_param("collapseRepeated", true),
            utilize_absolute: p.bool_param("utilizeAbsolute", true),
            leading_zero: p.bool_param("leadingZero", true),
            negative_extra_space: p.bool_param("negativeExtraSpace", true),
            no_space_after_flags: p.bool_param("noSpaceAfterFlags", false),
            force_absolute_path: p.bool_param("forceAbsolutePath", false),
        }
    }
    fn cleanup(&self) -> CleanupParams {
        CleanupParams {
            leading_zero: self.leading_zero,
            negative_extra_space: self.negative_extra_space,
            no_space_after_flags: self.no_space_after_flags,
        }
    }
}

struct Ctx {
    precision: Option<i32>,
    error: f64,
    arc_threshold: f64,
    arc_tolerance: f64,
}

impl Ctx {
    fn round_data(&self, data: &mut [f64]) {
        match self.precision {
            Some(p) if p > 0 && p < 20 => strong_round(data, p, self.error),
            _ => {
                for v in data.iter_mut() {
                    *v = crate::tools::js_round(*v);
                }
            }
        }
    }
}

fn strong_round(data: &mut [f64], precision: i32, error: f64) {
    for i in (0..data.len()).rev() {
        let fixed = to_fixed(data[i], precision);
        if fixed != data[i] {
            let rounded = to_fixed(data[i], precision - 1);
            data[i] = if to_fixed((rounded - data[i]).abs(), precision + 1) >= error {
                fixed
            } else {
                rounded
            };
        }
    }
}

pub fn convert_path_data(doc: &mut Document, params_json: &Json) {
    let params = Params::from_json(params_json);
    if params.apply_transforms {
        apply_transforms(doc, params.transform_precision, params.apply_transforms_stroked);
    }
    let sheet = collect_stylesheet(doc);
    let mut ancestors: Vec<Element> = Vec::new();
    convert_children(&mut doc.children, &sheet, &params, &mut ancestors);
}

fn compute_with_ancestors(
    sheet: &Stylesheet,
    el: &Element,
    ancestors: &[Element],
    prop: &str,
) -> Option<String> {
    if let Some(v) = compute_prop(sheet, el, prop) {
        return Some(v);
    }
    for anc in ancestors.iter().rev() {
        if let Some(v) = compute_prop(sheet, anc, prop) {
            return Some(v);
        }
    }
    None
}

fn convert_children(
    nodes: &mut [Node],
    sheet: &Stylesheet,
    params: &Params,
    ancestors: &mut Vec<Element>,
) {
    for n in nodes.iter_mut() {
        if let Node::Element(el) = n {
            if ["path", "glyph", "missing-glyph"].contains(&el.name.as_str())
                && el.get("d").is_some()
            {
                convert_one_path(el, sheet, params, ancestors);
            }
            ancestors.push(Element {
                name: el.name.clone(),
                attrs: el.attrs.clone(),
                children: Vec::new(),
            });
            convert_children(&mut el.children, sheet, params, ancestors);
            ancestors.pop();
        }
    }
}

fn convert_one_path(el: &mut Element, sheet: &Stylesheet, params: &Params, ancestors: &[Element]) {
    let precision = params.float_precision;
    let error = match precision {
        Some(p) => to_fixed(10f64.powi(-p), p),
        None => 1e-2,
    };
    let ctx = Ctx {
        precision,
        error,
        arc_threshold: params.make_arcs.map(|m| m.0).unwrap_or(2.5),
        arc_tolerance: params.make_arcs.map(|m| m.1).unwrap_or(0.5),
    };
    let _ = (ctx.arc_threshold, ctx.arc_tolerance);
    let has_marker_mid = compute_with_ancestors(sheet, el, ancestors, "marker-mid").is_some();
    let stroke = compute_with_ancestors(sheet, el, ancestors, "stroke");
    let maybe_has_stroke = stroke.as_deref().map(|v| v != "none").unwrap_or(false);
    let linecap = compute_with_ancestors(sheet, el, ancestors, "stroke-linecap");
    let maybe_has_linecap = linecap.as_deref().map(|v| v != "butt").unwrap_or(false);
    let is_safe_to_use_z = if maybe_has_stroke {
        linecap.as_deref() == Some("round")
            && compute_with_ancestors(sheet, el, ancestors, "stroke-linejoin").as_deref()
                == Some("round")
    } else {
        true
    };
    let is_safe_to_remove = |is_first: bool, safe_if_not_first: bool| -> bool {
        if !maybe_has_stroke {
            true
        } else if is_first {
            !maybe_has_linecap
        } else {
            safe_if_not_first
        }
    };

    let d = el.get("d").unwrap_or("").to_string();
    let mut data = path2js(&d);
    if data.is_empty() {
        return;
    }
    let includes_vertices = data.iter().any(|i| i.command != 'm' && i.command != 'M');
    convert_to_relative(&mut data);
    data = filters(data, params, &ctx, is_safe_to_use_z, &is_safe_to_remove, has_marker_mid);
    if params.utilize_absolute {
        data = convert_to_mixed(data, params, &ctx);
    }
    let has_marker = el.has("marker-start") || el.has("marker-end");
    let markers_only = has_marker
        && includes_vertices
        && data.iter().all(|i| i.command == 'm' || i.command == 'M');
    if markers_only {
        data.push(PathItem::new('z', Vec::new()));
    }
    let s = js2path(&data, precision, params.no_space_after_flags);
    el.set("d", s);
}

// ---------- applyTransforms ----------
const REFERENCES_PROPS: &[&str] = &[
    "clip-path", "color-profile", "fill", "filter", "marker-end", "marker-mid", "marker-start",
    "mask", "stroke", "style",
];

fn apply_transforms(doc: &mut Document, transform_precision: i32, stroked: bool) {
    let sheet = collect_stylesheet(doc);
    let mut ancestors: Vec<Element> = Vec::new();
    apply_transforms_children(&mut doc.children, &sheet, transform_precision, stroked, &mut ancestors);
}

fn apply_transforms_children(
    nodes: &mut [Node],
    sheet: &Stylesheet,
    transform_precision: i32,
    stroked: bool,
    ancestors: &mut Vec<Element>,
) {
    for n in nodes.iter_mut() {
        if let Node::Element(el) = n {
            apply_transforms_to_el(el, sheet, transform_precision, stroked, ancestors);
            ancestors.push(Element {
                name: el.name.clone(),
                attrs: el.attrs.clone(),
                children: Vec::new(),
            });
            apply_transforms_children(&mut el.children, sheet, transform_precision, stroked, ancestors);
            ancestors.pop();
        }
    }
}

fn compute_with_sheet_and_ancestors(
    sheet: &Stylesheet,
    el: &Element,
    ancestors: &[Element],
    prop: &str,
) -> Option<String> {
    if let Some(v) = compute_prop(sheet, el, prop) {
        return Some(v);
    }
    for anc in ancestors.iter().rev() {
        if let Some(v) = compute_prop(sheet, anc, prop) {
            return Some(v);
        }
    }
    None
}

fn replace_numbers_scaled(s: &str, scale: f64) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() || c == '.' || ((c == '-' || c == '+') && i + 1 < chars.len() && (chars[i + 1].is_ascii_digit() || chars[i + 1] == '.')) {
            let start = i;
            let mut j = i;
            if chars[j] == '-' || chars[j] == '+' { j += 1; }
            while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') { j += 1; }
            if j < chars.len() && (chars[j] == 'e' || chars[j] == 'E') {
                let mut k = j + 1;
                if k < chars.len() && (chars[k] == '+' || chars[k] == '-') { k += 1; }
                if k < chars.len() && chars[k].is_ascii_digit() {
                    while k < chars.len() && chars[k].is_ascii_digit() { k += 1; }
                    j = k;
                }
            }
            let txt: String = chars[start..j].iter().collect();
            if let Ok(n) = txt.parse::<f64>() {
                out.push_str(&remove_leading_zero(n * scale));
            } else {
                out.push_str(&txt);
            }
            i = j;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn apply_transforms_to_el(
    el: &mut Element,
    sheet: &Stylesheet,
    transform_precision: i32,
    stroked: bool,
    ancestors: &[Element],
) {
    if el.get("d").is_none() {
        return;
    }
    if el.has("id") {
        return;
    }
    let transform = match el.get("transform") {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => return,
    };
    if el.has("style") {
        return;
    }
    let has_ref = el
        .attrs
        .iter()
        .any(|(k, v)| REFERENCES_PROPS.contains(&k.as_str()) && crate::tools::includes_url_reference(v));
    if has_ref {
        return;
    }
    if let Some(ts) = compute_prop(sheet, el, "transform") {
        if ts != transform {
            return;
        }
    }
    let transforms = transform2js(&transform);
    if transforms.is_empty() {
        return;
    }
    let matrix_item = transforms_multiply(&transforms);
    let matrix = matrix_item.data;
    if matrix.len() < 6 {
        return;
    }
    let stroke = compute_with_sheet_and_ancestors(sheet, el, ancestors, "stroke");
    let stroke_width = compute_with_sheet_and_ancestors(sheet, el, ancestors, "stroke-width");
    // dynamic styles (var()) are treated as blocking; our compute returns the raw string
    if stroke.as_deref().map(|v| v.contains("var(")).unwrap_or(false)
        || stroke_width.as_deref().map(|v| v.contains("var(")).unwrap_or(false)
    {
        return;
    }
    let scale = to_fixed((matrix[0] * matrix[0] + matrix[1] * matrix[1]).sqrt(), transform_precision);
    let scale_num = scale;
    if let Some(stroke_val) = &stroke {
        if stroke_val != "none" {
            if !stroked {
                return;
            }
            if (matrix[0] != matrix[3] || matrix[1] != -matrix[2])
                && (matrix[0] != -matrix[3] || matrix[1] != matrix[2])
            {
                return;
            }
            if scale_num != 1.0 && el.get("vector-effect") != Some("non-scaling-stroke") {
                let sw = stroke_width.clone().unwrap_or_else(|| "1".to_string());
                let new_sw = replace_numbers_scaled(sw.trim(), scale_num);
                el.set("stroke-width", new_sw);
                if let Some(v) = el.get("stroke-dashoffset").map(|s| s.to_string()) {
                    let nv = replace_numbers_scaled(v.trim(), scale_num);
                    el.set("stroke-dashoffset", nv);
                }
                if let Some(v) = el.get("stroke-dasharray").map(|s| s.to_string()) {
                    let nv = replace_numbers_scaled(v.trim(), scale_num);
                    el.set("stroke-dasharray", nv);
                }
            }
        }
    }
    let d = el.get("d").unwrap_or("").to_string();
    let mut path_data = path2js(&d);
    apply_matrix_to_path_data(&mut path_data, &matrix);
    // store back temporarily in d (will be re-parsed by convert), using full precision stringify
    let s = js2path(&path_data, None, false);
    el.set("d", s);
    el.remove_attr("transform");
}

fn t_abs(m: &[f64], x: f64, y: f64) -> [f64; 2] {
    [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]]
}
fn t_rel(m: &[f64], x: f64, y: f64) -> [f64; 2] {
    [m[0] * x + m[2] * y, m[1] * x + m[3] * y]
}

fn apply_matrix_to_path_data(path_data: &mut [PathItem], matrix: &[f64]) {
    let mut start = [0.0f64; 2];
    let mut cursor = [0.0f64; 2];
    for item in path_data.iter_mut() {
        let mut command = item.command;
        let mut args = item.args.clone();
        if command == 'M' {
            cursor = [args[0], args[1]];
            start = cursor;
            let p = t_abs(matrix, args[0], args[1]);
            args = vec![p[0], p[1]];
        }
        if command == 'm' {
            cursor = [cursor[0] + args[0], cursor[1] + args[1]];
            start = cursor;
            let p = t_rel(matrix, args[0], args[1]);
            args = vec![p[0], p[1]];
        }
        if command == 'H' { command = 'L'; args = vec![args[0], cursor[1]]; }
        if command == 'h' { command = 'l'; args = vec![args[0], 0.0]; }
        if command == 'V' { command = 'L'; args = vec![cursor[0], args[0]]; }
        if command == 'v' { command = 'l'; args = vec![0.0, args[0]]; }
        if command == 'L' {
            cursor = [args[0], args[1]];
            let p = t_abs(matrix, args[0], args[1]);
            args = vec![p[0], p[1]];
        }
        if command == 'l' {
            cursor = [cursor[0] + args[0], cursor[1] + args[1]];
            let p = t_rel(matrix, args[0], args[1]);
            args = vec![p[0], p[1]];
        }
        if command == 'C' {
            cursor = [args[4], args[5]];
            let p1 = t_abs(matrix, args[0], args[1]);
            let p2 = t_abs(matrix, args[2], args[3]);
            let p = t_abs(matrix, args[4], args[5]);
            args = vec![p1[0], p1[1], p2[0], p2[1], p[0], p[1]];
        }
        if command == 'c' {
            cursor = [cursor[0] + args[4], cursor[1] + args[5]];
            let p1 = t_rel(matrix, args[0], args[1]);
            let p2 = t_rel(matrix, args[2], args[3]);
            let p = t_rel(matrix, args[4], args[5]);
            args = vec![p1[0], p1[1], p2[0], p2[1], p[0], p[1]];
        }
        if command == 'S' {
            cursor = [args[2], args[3]];
            let p2 = t_abs(matrix, args[0], args[1]);
            let p = t_abs(matrix, args[2], args[3]);
            args = vec![p2[0], p2[1], p[0], p[1]];
        }
        if command == 's' {
            cursor = [cursor[0] + args[2], cursor[1] + args[3]];
            let p2 = t_rel(matrix, args[0], args[1]);
            let p = t_rel(matrix, args[2], args[3]);
            args = vec![p2[0], p2[1], p[0], p[1]];
        }
        if command == 'Q' {
            cursor = [args[2], args[3]];
            let p1 = t_abs(matrix, args[0], args[1]);
            let p = t_abs(matrix, args[2], args[3]);
            args = vec![p1[0], p1[1], p[0], p[1]];
        }
        if command == 'q' {
            cursor = [cursor[0] + args[2], cursor[1] + args[3]];
            let p1 = t_rel(matrix, args[0], args[1]);
            let p = t_rel(matrix, args[2], args[3]);
            args = vec![p1[0], p1[1], p[0], p[1]];
        }
        if command == 'T' {
            cursor = [args[0], args[1]];
            let p = t_abs(matrix, args[0], args[1]);
            args = vec![p[0], p[1]];
        }
        if command == 't' {
            cursor = [cursor[0] + args[0], cursor[1] + args[1]];
            let p = t_rel(matrix, args[0], args[1]);
            args = vec![p[0], p[1]];
        }
        if command == 'A' {
            let old_cursor = cursor;
            let mut a = args.clone();
            transform_arc(old_cursor, &mut a, matrix);
            cursor = [a[5], a[6]];
            if a[2].abs() > 80.0 {
                let r0 = a[0];
                let rotation = a[2];
                a[0] = a[1];
                a[1] = r0;
                a[2] = rotation + if rotation > 0.0 { -90.0 } else { 90.0 };
            }
            let p = t_abs(matrix, a[5], a[6]);
            a[5] = p[0];
            a[6] = p[1];
            args = a;
        }
        if command == 'a' {
            let mut a = args.clone();
            transform_arc([0.0, 0.0], &mut a, matrix);
            cursor = [cursor[0] + a[5], cursor[1] + a[6]];
            if a[2].abs() > 80.0 {
                let r0 = a[0];
                let rotation = a[2];
                a[0] = a[1];
                a[1] = r0;
                a[2] = rotation + if rotation > 0.0 { -90.0 } else { 90.0 };
            }
            let p = t_rel(matrix, a[5], a[6]);
            a[5] = p[0];
            a[6] = p[1];
            args = a;
        }
        if command == 'z' || command == 'Z' {
            cursor = start;
        }
        item.command = command;
        item.args = args;
    }
}

// ---------- convertToRelative ----------
fn convert_to_relative(path_data: &mut [PathItem]) {
    let mut start = [0.0f64; 2];
    let mut cursor = [0.0f64; 2];
    let mut prev_coords = [0.0f64; 2];
    for (i, item) in path_data.iter_mut().enumerate() {
        let mut command = item.command;
        let mut args = item.args.clone();
        match command {
            'm' => {
                cursor[0] += args[0];
                cursor[1] += args[1];
                start = cursor;
            }
            'M' => {
                if i != 0 {
                    command = 'm';
                }
                args[0] -= cursor[0];
                args[1] -= cursor[1];
                cursor[0] += args[0];
                cursor[1] += args[1];
                start = cursor;
            }
            'l' => {
                cursor[0] += args[0];
                cursor[1] += args[1];
            }
            'L' => {
                command = 'l';
                args[0] -= cursor[0];
                args[1] -= cursor[1];
                cursor[0] += args[0];
                cursor[1] += args[1];
            }
            'h' => cursor[0] += args[0],
            'H' => {
                command = 'h';
                args[0] -= cursor[0];
                cursor[0] += args[0];
            }
            'v' => cursor[1] += args[0],
            'V' => {
                command = 'v';
                args[0] -= cursor[1];
                cursor[1] += args[0];
            }
            'c' => {
                cursor[0] += args[4];
                cursor[1] += args[5];
            }
            'C' => {
                command = 'c';
                for k in 0..6 {
                    args[k] -= cursor[k % 2];
                }
                cursor[0] += args[4];
                cursor[1] += args[5];
            }
            's' => {
                cursor[0] += args[2];
                cursor[1] += args[3];
            }
            'S' => {
                command = 's';
                for k in 0..4 {
                    args[k] -= cursor[k % 2];
                }
                cursor[0] += args[2];
                cursor[1] += args[3];
            }
            'q' => {
                cursor[0] += args[2];
                cursor[1] += args[3];
            }
            'Q' => {
                command = 'q';
                for k in 0..4 {
                    args[k] -= cursor[k % 2];
                }
                cursor[0] += args[2];
                cursor[1] += args[3];
            }
            't' => {
                cursor[0] += args[0];
                cursor[1] += args[1];
            }
            'T' => {
                command = 't';
                args[0] -= cursor[0];
                args[1] -= cursor[1];
                cursor[0] += args[0];
                cursor[1] += args[1];
            }
            'a' => {
                cursor[0] += args[5];
                cursor[1] += args[6];
            }
            'A' => {
                command = 'a';
                args[5] -= cursor[0];
                args[6] -= cursor[1];
                cursor[0] += args[5];
                cursor[1] += args[6];
            }
            'Z' | 'z' => {
                cursor = start;
            }
            _ => {}
        }
        item.command = command;
        item.args = args;
        item.base = prev_coords;
        item.coords = cursor;
        prev_coords = item.coords;
    }
}

fn is_curve_straight_line(data: &[f64], error: f64) -> bool {
    if data.len() < 2 {
        return false;
    }
    let mut i = data.len() - 2;
    let a = -data[i + 1];
    let b = data[i];
    let d = 1.0 / (a * a + b * b);
    if i <= 1 || !d.is_finite() {
        return false;
    }
    while i >= 2 {
        i -= 2;
        if ((a * data[i] + b * data[i + 1]).powi(2) * d).sqrt() > error {
            return false;
        }
        if i == 0 {
            break;
        }
    }
    true
}

fn calculate_sagitta(data: &[f64], error: f64) -> Option<f64> {
    if data.len() < 7 || data[3] == 1.0 {
        return None;
    }
    let (rx, ry) = (data[0], data[1]);
    if (rx - ry).abs() > error {
        return None;
    }
    let chord = data[5].hypot(data[6]);
    if chord > rx * 2.0 {
        return None;
    }
    Some(rx - (rx * rx - 0.25 * chord * chord).sqrt())
}

fn get_intersection(coords: &[f64; 8]) -> Option<[f64; 2]> {
    let a1 = coords[1] - coords[3];
    let b1 = coords[2] - coords[0];
    let c1 = coords[0] * coords[3] - coords[2] * coords[1];
    let a2 = coords[5] - coords[7];
    let b2 = coords[6] - coords[4];
    let c2 = coords[4] * coords[7] - coords[5] * coords[6];
    let denom = a1 * b2 - a2 * b1;
    if denom == 0.0 {
        return None;
    }
    let cross_pt = [(b1 * c2 - b2 * c1) / denom, (a1 * c2 - a2 * c1) / -denom];
    if !cross_pt[0].is_finite() || !cross_pt[1].is_finite() {
        return None;
    }
    Some(cross_pt)
}

fn is_convex(data: &[f64]) -> bool {
    if data.len() < 6 {
        return false;
    }
    let center = match get_intersection(&[0.0, 0.0, data[2], data[3], data[0], data[1], data[4], data[5]]) {
        Some(c) => c,
        None => return false,
    };
    (data[2] < center[0]) == (center[0] < 0.0)
        && (data[3] < center[1]) == (center[1] < 0.0)
        && (data[4] < center[0]) == (center[0] < data[0])
        && (data[5] < center[1]) == (center[1] < data[1])
}

fn get_cubic_point(curve: &[f64], t: f64) -> [f64; 2] {
    let sqr_t = t * t;
    let cub_t = sqr_t * t;
    let mt = 1.0 - t;
    let sqr_mt = mt * mt;
    [
        3.0 * sqr_mt * t * curve[0] + 3.0 * mt * sqr_t * curve[2] + cub_t * curve[4],
        3.0 * sqr_mt * t * curve[1] + 3.0 * mt * sqr_t * curve[3] + cub_t * curve[5],
    ]
}

fn get_distance(p1: [f64; 2], p2: [f64; 2]) -> f64 {
    (p1[0] - p2[0]).hypot(p1[1] - p2[1])
}

struct Circle {
    center: [f64; 2],
    radius: f64,
}

fn find_circle(curve: &[f64], ctx: &Ctx) -> Option<Circle> {
    let mid = get_cubic_point(curve, 0.5);
    let m1 = [mid[0] / 2.0, mid[1] / 2.0];
    let m2 = [(mid[0] + curve[4]) / 2.0, (mid[1] + curve[5]) / 2.0];
    let center = get_intersection(&[
        m1[0], m1[1], m1[0] + m1[1], m1[1] - m1[0], m2[0], m2[1],
        m2[0] + (m2[1] - mid[1]), m2[1] - (m2[0] - mid[0]),
    ])?;
    let radius = get_distance([0.0, 0.0], center);
    let tolerance = (ctx.arc_threshold * ctx.error).min(ctx.arc_tolerance * radius / 100.0);
    if radius < 1e15
        && [0.25, 0.75].iter().all(|t| {
            (get_distance(get_cubic_point(curve, *t), center) - radius).abs() <= tolerance
        })
    {
        Some(Circle { center, radius })
    } else {
        None
    }
}

fn is_arc(curve: &[f64], circle: &Circle, ctx: &Ctx) -> bool {
    let tolerance = (ctx.arc_threshold * ctx.error).min(ctx.arc_tolerance * circle.radius / 100.0);
    [0.0, 0.25, 0.5, 0.75, 1.0].iter().all(|t| {
        (get_distance(get_cubic_point(curve, *t), circle.center) - circle.radius).abs() <= tolerance
    })
}

fn is_arc_prev(curve: &[f64], circle: &Circle, ctx: &Ctx) -> bool {
    is_arc(
        curve,
        &Circle {
            center: [circle.center[0] + curve[4], circle.center[1] + curve[5]],
            radius: circle.radius,
        },
        ctx,
    )
}

fn find_arc_angle(curve: &[f64], rel_circle: &Circle) -> f64 {
    let x1 = -rel_circle.center[0];
    let y1 = -rel_circle.center[1];
    let x2 = curve[4] - rel_circle.center[0];
    let y2 = curve[5] - rel_circle.center[1];
    ((x1 * x2 + y1 * y2) / ((x1 * x1 + y1 * y1) * (x2 * x2 + y2 * y2)).sqrt()).acos()
}

fn reflect_point(cp: [f64; 2], base: [f64; 2]) -> [f64; 2] {
    [2.0 * base[0] - cp[0], 2.0 * base[1] - cp[1]]
}

fn data2_path(params: &Params, ctx: &Ctx, items: &[PathItem]) -> String {
    let mut out = String::new();
    for item in items {
        let mut args = item.args.clone();
        ctx.round_data(&mut args);
        out.push(item.command);
        out.push_str(&cleanup_out_data(&args, &params.cleanup(), Some(item.command)));
    }
    out
}

// ---------- filters ----------
fn filters(
    mut path: Vec<PathItem>,
    params: &Params,
    ctx: &Ctx,
    is_safe_to_use_z: bool,
    is_safe_to_remove: &dyn Fn(bool, bool) -> bool,
    has_marker_mid: bool,
) -> Vec<PathItem> {
    let _ = data2_path; // used by makeArcs in JS; arc conversion not ported (see unresolved)
    let mut out: Vec<PathItem> = Vec::new();
    let mut rel_subpoint = [0.0f64; 2];
    let mut path_base = [0.0f64; 2];
    let mut prev: Option<PathItem> = None;
    let mut prev_q_control: Option<[f64; 2]> = None;
    let mut idx = 0usize;
    while idx < path.len() {
        let q_control_point = prev_q_control;
        let mut item = path[idx].clone();
        let mut command = item.command;
        let mut data = item.args.clone();
        let next_command = path.get(idx + 1).map(|n| n.command);

        if command != 'Z' && command != 'z' {
            // sdata for s
            let mut sdata: Vec<f64> = data.clone();
            if command == 's' {
                sdata = vec![0.0, 0.0];
                sdata.extend(data.iter().cloned());
                if let Some(p) = &prev {
                    let pd = &p.args;
                    let n = pd.len();
                    if n >= 4 {
                        sdata[0] = pd[n - 2] - pd[n - 4];
                        sdata[1] = pd[n - 1] - pd[n - 3];
                    }
                }
            }

            // convert curves to arcs if possible (port of makeArcs block)
            if params.make_arcs.is_some()
                && (command == 'c' || command == 's')
                && is_convex(&sdata)
            {
                if let Some(circle) = find_circle(&sdata, ctx) {
                    let mut r_vec = vec![circle.radius];
                    ctx.round_data(&mut r_vec);
                    let r = r_vec[0];
                    let mut angle = find_arc_angle(&sdata, &circle);
                    let sweep = if sdata[5] * sdata[0] - sdata[4] * sdata[1] > 0.0 {
                        1.0
                    } else {
                        0.0
                    };
                    #[derive(Clone)]
                    struct ArcItem {
                        args: Vec<f64>,
                        coords: [f64; 2],
                        base: [f64; 2],
                    }
                    let mut arc = ArcItem {
                        args: vec![r, r, 0.0, 0.0, sweep, sdata[4], sdata[5]],
                        coords: item.coords,
                        base: item.base,
                    };
                    let mut output: Vec<ArcItem> = vec![arc.clone()];
                    let mut rel_center = [circle.center[0] - sdata[4], circle.center[1] - sdata[5]];
                    let mut arc_curves: Vec<(char, Vec<f64>)> =
                        vec![(command, data.clone())];
                    let mut has_prev = false;
                    let mut suffix = String::new();
                    let mut j_opt: Option<usize> = None;

                    // previous curve fits same circle?
                    if let Some(p) = &prev {
                        let prev_fits = (p.command == 'c'
                            && is_convex(&p.args)
                            && is_arc_prev(&p.args, &circle, ctx))
                            || (p.command == 'a'
                                && p.sdata.is_some()
                                && is_arc_prev(p.sdata.as_ref().unwrap(), &circle, ctx));
                        if prev_fits {
                            arc_curves.insert(0, (p.command, p.args.clone()));
                            arc.base = p.base;
                            arc.args[5] = arc.coords[0] - arc.base[0];
                            arc.args[6] = arc.coords[1] - arc.base[1];
                            output[0] = arc.clone();
                            let prev_data = if p.command == 'a' {
                                p.sdata.clone().unwrap()
                            } else {
                                p.args.clone()
                            };
                            let prev_angle = find_arc_angle(
                                &prev_data,
                                &Circle {
                                    center: [
                                        prev_data[4] + circle.center[0],
                                        prev_data[5] + circle.center[1],
                                    ],
                                    radius: circle.radius,
                                },
                            );
                            angle += prev_angle;
                            if angle > std::f64::consts::PI {
                                arc.args[3] = 1.0;
                                output[0].args[3] = 1.0;
                            }
                            has_prev = true;
                        }
                    }

                    // following curves fitting the arc
                    let mut j = idx + 1;
                    let mut consumed_to: Option<usize> = None;
                    while j < path.len() && (path[j].command == 'c' || path[j].command == 's') {
                        let next_item = path[j].clone();
                        let mut next_data = next_item.args.clone();
                        let mut suffix_item: Option<(char, Vec<f64>)> = None;
                        if next_item.command == 's' {
                            let prev_args = path[j - 1].args.clone();
                            let mut lh = next_item.clone();
                            longhand(&mut lh, &prev_args);
                            next_data = lh.args.clone();
                            let truncated = next_data[..2].to_vec();
                            suffix_item = Some((lh.command, truncated));
                        }
                        if let Some(si) = &suffix_item {
                            suffix = {
                                let mut a = si.1.clone();
                                ctx.round_data(&mut a);
                                format!(
                                    "{}{}",
                                    si.0,
                                    cleanup_out_data(&a, &params.cleanup(), Some(si.0))
                                )
                            };
                        }
                        let rel_circle = Circle { center: rel_center, radius: circle.radius };
                        if is_convex(&next_data) && is_arc(&next_data, &rel_circle, ctx) {
                            angle += find_arc_angle(&next_data, &rel_circle);
                            if angle - 2.0 * std::f64::consts::PI > 1e-3 {
                                j_opt = Some(j);
                                break;
                            }
                            if angle > std::f64::consts::PI {
                                arc.args[3] = 1.0;
                            }
                            arc_curves.push((next_item.command, next_item.args.clone()));
                            if 2.0 * std::f64::consts::PI - angle > 1e-3 {
                                arc.coords = next_item.coords;
                                arc.args[5] = arc.coords[0] - arc.base[0];
                                arc.args[6] = arc.coords[1] - arc.base[1];
                                // update the working arc in output (last element before any full-circle append)
                                if let Some(last) = output.last_mut() {
                                    *last = arc.clone();
                                }
                            } else {
                                arc.args[5] = 2.0 * (rel_center[0] - next_data[4]);
                                arc.args[6] = 2.0 * (rel_center[1] - next_data[5]);
                                arc.coords = [
                                    arc.base[0] + arc.args[5],
                                    arc.base[1] + arc.args[6],
                                ];
                                if let Some(last) = output.last_mut() {
                                    *last = arc.clone();
                                }
                                let new_arc = ArcItem {
                                    args: vec![
                                        r,
                                        r,
                                        0.0,
                                        0.0,
                                        sweep,
                                        next_item.coords[0] - arc.coords[0],
                                        next_item.coords[1] - arc.coords[1],
                                    ],
                                    coords: next_item.coords,
                                    base: arc.coords,
                                };
                                output.push(new_arc);
                                j += 1;
                                consumed_to = Some(j);
                                j_opt = Some(j);
                                break;
                            }
                            rel_center[0] -= next_data[4];
                            rel_center[1] -= next_data[5];
                            j += 1;
                            consumed_to = Some(j);
                        } else {
                            j_opt = Some(j);
                            break;
                        }
                    }
                    if j_opt.is_none() {
                        j_opt = Some(j);
                    }

                    let stringify_items = |items: &[(char, Vec<f64>)]| -> String {
                        let mut out_s = String::new();
                        for (cmd, args) in items {
                            let mut a = args.clone();
                            ctx.round_data(&mut a);
                            out_s.push(*cmd);
                            out_s.push_str(&cleanup_out_data(&a, &params.cleanup(), Some(*cmd)));
                        }
                        out_s
                    };
                    let out_items: Vec<(char, Vec<f64>)> =
                        output.iter().map(|a| ('a', a.args.clone())).collect();
                    let curves_items: Vec<(char, Vec<f64>)> = arc_curves.clone();
                    if (stringify_items(&out_items).len() + suffix.len())
                        < stringify_items(&curves_items).len()
                    {
                        if let Some(jj) = j_opt {
                            if jj < path.len() && path[jj].command == 's' && jj >= 1 {
                                let prev_args = path[jj - 1].args.clone();
                                longhand(&mut path[jj], &prev_args);
                            }
                        }
                        if has_prev {
                            let prev_arc = output.remove(0);
                            let mut pa = prev_arc.args.clone();
                            ctx.round_data(&mut pa);
                            if let Some(p) = &prev {
                                let n = p.args.len();
                                if n >= 2 {
                                    rel_subpoint[0] += pa[5] - p.args[n - 2];
                                    rel_subpoint[1] += pa[6] - p.args[n - 1];
                                }
                            }
                            // mutate prev (and its copy in out)
                            let new_prev_coords = prev_arc.coords;
                            if let Some(p) = &mut prev {
                                p.command = 'a';
                                p.args = pa.clone();
                                p.coords = new_prev_coords;
                            }
                            if let Some(last) = out.last_mut() {
                                last.command = 'a';
                                last.args = pa;
                                last.coords = new_prev_coords;
                            }
                            item.base = new_prev_coords;
                        }
                        let arc_opt = if output.is_empty() {
                            None
                        } else {
                            Some(output.remove(0))
                        };
                        if arc_curves.len() == 1 {
                            item.sdata = Some(sdata.clone());
                        } else if arc_curves.len() - 1 - (if has_prev { 1 } else { 0 }) > 0 {
                            let consumed = arc_curves.len() - 1 - (if has_prev { 1 } else { 0 });
                            let rest: Vec<PathItem> = output
                                .iter()
                                .map(|a| PathItem {
                                    command: 'a',
                                    args: a.args.clone(),
                                    base: a.base,
                                    coords: a.coords,
                                    sdata: None,
                                })
                                .collect();
                            // splice: replace `consumed` items after idx with rest
                            let start = idx + 1;
                            let avail = path.len().saturating_sub(start);
                            let remove_n = consumed.min(avail);
                            path.splice(start..start + remove_n, rest);
                            let _ = consumed_to;
                        }
                        match arc_opt {
                            None => {
                                idx += 1;
                                continue;
                            }
                            Some(arc_item) => {
                                command = 'a';
                                data = arc_item.args.clone();
                                item.coords = arc_item.coords;
                                // sync arc into item for later stages
                            }
                        }
                    }
                }
            }

            // rounding with accumulating error
            if ctx.precision.is_some() {
                if ['m', 'l', 't', 'q', 's', 'c'].contains(&command) {
                    for i in 0..data.len() {
                        data[i] += item.base[i % 2] - rel_subpoint[i % 2];
                    }
                } else if command == 'h' {
                    data[0] += item.base[0] - rel_subpoint[0];
                } else if command == 'v' {
                    data[0] += item.base[1] - rel_subpoint[1];
                } else if command == 'a' {
                    data[5] += item.base[0] - rel_subpoint[0];
                    data[6] += item.base[1] - rel_subpoint[1];
                }
                ctx.round_data(&mut data);
                if command == 'h' {
                    rel_subpoint[0] += data[0];
                } else if command == 'v' {
                    rel_subpoint[1] += data[0];
                } else if data.len() >= 2 {
                    rel_subpoint[0] += data[data.len() - 2];
                    rel_subpoint[1] += data[data.len() - 1];
                }
                ctx.round_data(&mut rel_subpoint);
                if command == 'M' || command == 'm' {
                    path_base = rel_subpoint;
                }
            }

            let sagitta = if command == 'a' {
                calculate_sagitta(&data, ctx.error)
            } else {
                None
            };
            if params.smart_arc_rounding && sagitta.is_some() && ctx.precision.is_some() {
                let prec = ctx.precision.unwrap();
                let sag = sagitta.unwrap();
                for pn in (0..=prec).rev() {
                    let radius = to_fixed(data[0], pn);
                    let mut nd = vec![radius, radius];
                    nd.extend(data[2..].iter().cloned());
                    match calculate_sagitta(&nd, ctx.error) {
                        Some(sag_new) if (sag - sag_new).abs() < ctx.error => {
                            data[0] = radius;
                            data[1] = radius;
                        }
                        _ => break,
                    }
                }
            }

            // straight curves
            if params.straight_curves {
                if (command == 'c' && is_curve_straight_line(&data, ctx.error))
                    || (command == 's' && is_curve_straight_line(&sdata, ctx.error))
                {
                    if next_command == Some('s') {
                        let cur = data.clone();
                        if let Some(nx) = path.get_mut(idx + 1) {
                            longhand(nx, &cur);
                        }
                    }
                    command = 'l';
                    data = data[data.len() - 2..].to_vec();
                } else if (command == 'q' && is_curve_straight_line(&data, ctx.error))
                    || (command == 't'
                        && prev
                            .as_ref()
                            .map(|p| p.command != 'q' && p.command != 't')
                            .unwrap_or(true))
                {
                    if command == 'q' && next_command == Some('t') {
                        let cur = data.clone();
                        if let Some(nx) = path.get_mut(idx + 1) {
                            longhand(nx, &cur);
                        }
                    }
                    if command == 't' && next_command == Some('t') {
                        let coords = item.coords;
                        let base = item.base;
                        if let Some(nx) = path.get_mut(idx + 1) {
                            nx.command = 'q';
                            nx.args.insert(0, (2.0 * coords[1] - base[1]) - coords[1]);
                            nx.args.insert(0, (2.0 * coords[0] - base[0]) - coords[0]);
                        }
                    }
                    command = 'l';
                    data = data[data.len() - 2..].to_vec();
                } else if command == 'a'
                    && (data[0] == 0.0
                        || data[1] == 0.0
                        || sagitta.map(|s| s < ctx.error).unwrap_or(false))
                {
                    command = 'l';
                    data = data[data.len() - 2..].to_vec();
                }
            }

            // convertToQ
            if params.convert_to_q && command == 'c' && data.len() == 6 {
                let base = item.base;
                let x1 = 0.75 * (base[0] + data[0]) - 0.25 * base[0];
                let x2 = 0.75 * (base[0] + data[2]) - 0.25 * (base[0] + data[4]);
                if (x1 - x2).abs() < ctx.error * 2.0 {
                    let y1 = 0.75 * (base[1] + data[1]) - 0.25 * base[1];
                    let y2 = 0.75 * (base[1] + data[3]) - 0.25 * (base[1] + data[5]);
                    if (y1 - y2).abs() < ctx.error * 2.0 {
                        let mut new_data = vec![x1 + x2 - base[0], y1 + y2 - base[1], data[4], data[5]];
                        ctx.round_data(&mut new_data);
                        let cp = params.cleanup();
                        let orig_len = cleanup_out_data(&data, &cp, Some(command)).len();
                        let new_len = cleanup_out_data(&new_data, &cp, Some('q')).len();
                        if new_len < orig_len {
                            // fix up next curve: an s reflects the cubic's second control
                            // point, which the quadratic no longer carries
                            if next_command == Some('s') {
                                let cubic = data.clone();
                                if let Some(nx) = path.get_mut(idx + 1) {
                                    longhand(nx, &cubic);
                                }
                            }
                            command = 'q';
                            data = new_data;
                        }
                    }
                }
            }

            // line shorthands
            if params.line_shorthands && command == 'l' && data.len() == 2 {
                if data[1] == 0.0 {
                    command = 'h';
                    data.pop();
                } else if data[0] == 0.0 {
                    command = 'v';
                    data.remove(0);
                }
            }

            // collapse repeated
            if params.collapse_repeated && !has_marker_mid {
                if let Some(p) = &prev {
                    if ['m', 'h', 'v'].contains(&command)
                        && p.command.to_ascii_lowercase() == command
                        && (command != 'h' && command != 'v'
                            || (p.args[0] >= 0.0) == (data[0] >= 0.0))
                    {
                        if let Some(last) = out.last_mut() {
                            last.args[0] += data[0];
                            if command != 'h' && command != 'v' {
                                last.args[1] += data[1];
                            }
                            last.coords = item.coords;
                            prev = Some(last.clone());
                        }
                        idx += 1;
                        continue;
                    }
                }
            }

            // curve smooth shorthands
            if params.curve_smooth_shorthands {
                if let Some(p) = &prev {
                    if command == 'c' && data.len() == 6 {
                        let pa = &p.args;
                        if p.command == 'c'
                            && pa.len() == 6
                            && (data[0] - -(pa[2] - pa[4])).abs() < ctx.error
                            && (data[1] - -(pa[3] - pa[5])).abs() < ctx.error
                        {
                            command = 's';
                            data = data[2..].to_vec();
                        } else if p.command == 's'
                            && pa.len() == 4
                            && (data[0] - -(pa[0] - pa[2])).abs() < ctx.error
                            && (data[1] - -(pa[1] - pa[3])).abs() < ctx.error
                        {
                            command = 's';
                            data = data[2..].to_vec();
                        } else if p.command != 'c'
                            && p.command != 's'
                            && data[0].abs() < ctx.error
                            && data[1].abs() < ctx.error
                        {
                            command = 's';
                            data = data[2..].to_vec();
                        }
                    } else if command == 'q' && data.len() == 4 {
                        let pa = &p.args;
                        if p.command == 'q'
                            && pa.len() == 4
                            && (data[0] - (pa[2] - pa[0])).abs() < ctx.error
                            && (data[1] - (pa[3] - pa[1])).abs() < ctx.error
                        {
                            command = 't';
                            data = data[2..].to_vec();
                        } else if p.command == 't' {
                            if let Some(qcp) = q_control_point {
                                let predicted = reflect_point(qcp, item.base);
                                let real = [data[0] + item.base[0], data[1] + item.base[1]];
                                if (predicted[0] - real[0]).abs() < ctx.error
                                    && (predicted[1] - real[1]).abs() < ctx.error
                                {
                                    command = 't';
                                    data = data[2..].to_vec();
                                }
                            }
                        }
                    }
                }
            }

            // remove useless
            if params.remove_useless {
                let prev_is_moveto = prev
                    .as_ref()
                    .map(|p| p.command == 'm' || p.command == 'M')
                    .unwrap_or(false);
                if is_safe_to_remove(prev_is_moveto, true) {
                    if ['l', 'h', 'v', 'q', 't', 'c', 's'].contains(&command)
                        && data.iter().all(|v| *v == 0.0)
                    {
                        idx += 1;
                        continue;
                    }
                    if command == 'a' && data.len() >= 7 && data[5] == 0.0 && data[6] == 0.0 {
                        idx += 1;
                        continue;
                    }
                }
            }

            // convert to z
            if params.convert_to_z
                && (is_safe_to_use_z || next_command == Some('Z') || next_command == Some('z'))
                && ['l', 'h', 'v'].contains(&command)
                && (path_base[0] - item.coords[0]).abs() < ctx.error
                && (path_base[1] - item.coords[1]).abs() < ctx.error
            {
                command = 'z';
                data = Vec::new();
            }

            item.command = command;
            item.args = data.clone();
        } else {
            rel_subpoint = path_base;
            if let Some(p) = &prev {
                if p.command == 'Z' || p.command == 'z' {
                    idx += 1;
                    continue;
                }
            }
        }

        // remove useless z
        if (command == 'Z' || command == 'z') && params.remove_useless {
            let prev_is_moveto = prev
                .as_ref()
                .map(|p| p.command == 'm' || p.command == 'M')
                .unwrap_or(false);
            if is_safe_to_remove(prev_is_moveto, is_safe_to_use_z)
                && (item.base[0] - item.coords[0]).abs() < ctx.error / 10.0
                && (item.base[1] - item.coords[1]).abs() < ctx.error / 10.0
            {
                idx += 1;
                continue;
            }
        }

        if command == 'q' {
            prev_q_control = Some([data[0] + item.base[0], data[1] + item.base[1]]);
        } else if command == 't' {
            if let Some(qcp) = q_control_point {
                prev_q_control = Some(reflect_point(qcp, item.base));
            } else {
                prev_q_control = Some(item.coords);
            }
        } else {
            prev_q_control = None;
        }

        item.command = command;
        if command != 'Z' && command != 'z' {
            item.args = data;
        }
        prev = Some(item.clone());
        out.push(item);
        idx += 1;
    }
    out
}

fn longhand(item: &mut PathItem, data: &[f64]) {
    match item.command {
        's' => item.command = 'c',
        't' => item.command = 'q',
        _ => {}
    }
    let n = data.len();
    if n >= 4 {
        let x = data[n - 2] - data[n - 4];
        let y = data[n - 1] - data[n - 3];
        item.args.splice(0..0, [x, y]);
    }
}

// ---------- convertToMixed ----------
fn convert_to_mixed(path: Vec<PathItem>, params: &Params, ctx: &Ctx) -> Vec<PathItem> {
    let mut result: Vec<PathItem> = Vec::new();
    if path.is_empty() {
        return result;
    }
    let mut prev = path[0].clone();
    for (index, item_in) in path.into_iter().enumerate() {
        if index == 0 {
            result.push(item_in);
            continue;
        }
        let mut item = item_in;
        if item.command == 'Z' || item.command == 'z' {
            prev = item.clone();
            result.push(item);
            continue;
        }
        let command = item.command;
        let data = item.args.clone();
        let mut adata = data.clone();
        let rdata = data.clone();
        if ['m', 'l', 't', 'q', 's', 'c'].contains(&command) {
            for i in 0..adata.len() {
                adata[i] += item.base[i % 2];
            }
        } else if command == 'h' {
            adata[0] += item.base[0];
        } else if command == 'v' {
            adata[0] += item.base[1];
        } else if command == 'a' {
            adata[5] += item.base[0];
            adata[6] += item.base[1];
        }
        ctx.round_data(&mut adata);
        let mut rdata2 = rdata.clone();
        ctx.round_data(&mut rdata2);
        let cp = params.cleanup();
        let abs_str = cleanup_out_data(&adata, &cp, Some(command));
        let rel_str = cleanup_out_data(&rdata2, &cp, Some(command));
        let convert = params.force_absolute_path
            || (abs_str.len() < rel_str.len()
                && !(params.negative_extra_space
                    && command == prev.command
                    && (prev.command as u32) > 96
                    && abs_str.len() == rel_str.len().saturating_sub(1)
                    && (data[0] < 0.0
                        || (data[0].floor() == 0.0
                            && data[0].fract() != 0.0
                            && prev
                                .args
                                .last()
                                .map(|v| v % 1.0 != 0.0)
                                .unwrap_or(false)))));
        if convert {
            item.command = command.to_ascii_uppercase();
            item.args = adata;
        }
        prev = item.clone();
        result.push(item);
    }
    result
}
