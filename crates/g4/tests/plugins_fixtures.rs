use svgo_rs_g4::parser::parse_svg;
use svgo_rs_g4::plugins::convert_ellipse_to_circle::ConvertEllipseToCircle;
use svgo_rs_g4::plugins::convert_shape_to_path::{ConvertShapeToPath, ConvertShapeToPathParams};
use svgo_rs_g4::plugins::move_elems_attrs_to_group::MoveElemsAttrsToGroup;
use svgo_rs_g4::plugins::move_group_attrs_to_elems::MoveGroupAttrsToElems;
use svgo_rs_g4::plugins::remove_empty_text::{RemoveEmptyText, RemoveEmptyTextParams};
use svgo_rs_g4::plugins::remove_hidden_elems::{RemoveHiddenElems, RemoveHiddenElemsParams};
use svgo_rs_g4::plugins::Plugin;
use svgo_rs_g4::stringifier::stringify_svg;
use std::path::PathBuf;

fn normalize(s: &str) -> String { s.trim().replace("\r\n", "\n") }
fn split_re(s: &str, delim: &str) -> Vec<String> {
    // emulate JS split(/\s*delim\s*/) : split on delim surrounded by optional whitespace
    let mut parts = Vec::new();
    let mut rest = s;
    loop {
        if let Some(pos) = rest.find(delim) {
            parts.push(rest[..pos].trim_end().to_string());
            rest = rest[pos+delim.len()..].trim_start();
        } else { parts.push(rest.to_string()); break; }
    }
    parts
}
fn param_bool(params: &str, key: &str) -> Option<bool> {
    let pat = format!("\"{}\"", key);
    let idx = params.find(&pat)?;
    let rest = &params[idx+pat.len()..];
    let rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c==':');
    if rest.starts_with("true") { Some(true) } else if rest.starts_with("false") { Some(false) } else { None }
}
fn param_num(params: &str, key: &str) -> Option<f64> {
    let pat = format!("\"{}\"", key);
    let idx = params.find(&pat)?;
    let rest = &params[idx+pat.len()..];
    let rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c==':');
    let mut end = 0;
    for (i,c) in rest.char_indices() { if c.is_ascii_digit()||c=='.'||c=='-'||c=='+'||c=='e'||c=='E' { end=i+c.len_utf8(); } else { break; } }
    rest[..end].parse::<f64>().ok()
}
fn apply_plugin(name: &str, params: &str, root: &mut svgo_rs_g4::xast::Root) {
    match name {
        "convertEllipseToCircle" => ConvertEllipseToCircle.apply(root),
        "moveElemsAttrsToGroup" => MoveElemsAttrsToGroup.apply(root),
        "moveGroupAttrsToElems" => MoveGroupAttrsToElems.apply(root),
        "removeEmptyText" => {
            let d = RemoveEmptyTextParams::default();
            let p = RemoveEmptyTextParams { text: param_bool(params,"text").unwrap_or(d.text), tspan: param_bool(params,"tspan").unwrap_or(d.tspan), tref: param_bool(params,"tref").unwrap_or(d.tref) };
            RemoveEmptyText { params: p }.apply(root);
        }
        "convertShapeToPath" => {
            let p = ConvertShapeToPathParams { convert_arcs: param_bool(params,"convertArcs").unwrap_or(false), float_precision: param_num(params,"floatPrecision").map(|v| v as i32) };
            ConvertShapeToPath { params: p }.apply(root);
        }
        "removeHiddenElems" => {
            let d = RemoveHiddenElemsParams::default();
            let p = RemoveHiddenElemsParams {
                is_hidden: param_bool(params,"isHidden").unwrap_or(d.is_hidden),
                display_none: param_bool(params,"displayNone").unwrap_or(d.display_none),
                opacity0: param_bool(params,"opacity0").unwrap_or(d.opacity0),
                circle_r0: param_bool(params,"circleR0").unwrap_or(d.circle_r0),
                ellipse_rx0: param_bool(params,"ellipseRX0").unwrap_or(d.ellipse_rx0),
                ellipse_ry0: param_bool(params,"ellipseRY0").unwrap_or(d.ellipse_ry0),
                rect_width0: param_bool(params,"rectWidth0").unwrap_or(d.rect_width0),
                rect_height0: param_bool(params,"rectHeight0").unwrap_or(d.rect_height0),
                pattern_width0: param_bool(params,"patternWidth0").unwrap_or(d.pattern_width0),
                pattern_height0: param_bool(params,"patternHeight0").unwrap_or(d.pattern_height0),
                image_width0: param_bool(params,"imageWidth0").unwrap_or(d.image_width0),
                image_height0: param_bool(params,"imageHeight0").unwrap_or(d.image_height0),
                path_empty_d: param_bool(params,"pathEmptyD").unwrap_or(d.path_empty_d),
                polyline_empty_points: param_bool(params,"polylineEmptyPoints").unwrap_or(d.polyline_empty_points),
                polygon_empty_points: param_bool(params,"polygonEmptyPoints").unwrap_or(d.polygon_empty_points),
            };
            RemoveHiddenElems { params: p }.apply(root);
        }
        other => panic!("unknown plugin {}", other),
    }
}
fn run_fixture(plugin: &str, file: &str) {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../reference/svgo/test/plugins"); path.push(file);
    let data = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {:?}: {}", path, e));
    let normalized = normalize(&data);
    let items = split_re(&normalized, "===");
    let test = if items.len()==2 { &items[1] } else { &items[0] };
    let parts = split_re(test, "@@@");
    assert!(parts.len()>=2, "{}: bad fixture format", file);
    let original = &parts[0]; let should = &parts[1];
    let params = if parts.len()>2 { parts[2].as_str() } else { "{}" };
    let mut last = original.to_string();
    for _ in 0..2 {
        let mut root = parse_svg(&last);
        apply_plugin(plugin, params, &mut root);
        last = stringify_svg(&root);
    }
    assert_eq!(normalize(&last), normalize(should), "\nfixture {} plugin {}\n--- expected ---\n{}\n--- got ---\n{}", file, plugin, should, last);
}
macro_rules! fx { ($name:ident, $plugin:expr, $file:expr) => { #[test] fn $name() { run_fixture($plugin, $file); } }; }
fx!(rh01, "removeHiddenElems", "removeHiddenElems.01.svg.txt");
fx!(rh02, "removeHiddenElems", "removeHiddenElems.02.svg.txt");
fx!(rh03, "removeHiddenElems", "removeHiddenElems.03.svg.txt");
fx!(rh04, "removeHiddenElems", "removeHiddenElems.04.svg.txt");
fx!(rh05, "removeHiddenElems", "removeHiddenElems.05.svg.txt");
fx!(rh06, "removeHiddenElems", "removeHiddenElems.06.svg.txt");
fx!(rh07, "removeHiddenElems", "removeHiddenElems.07.svg.txt");
fx!(rh08, "removeHiddenElems", "removeHiddenElems.08.svg.txt");
fx!(rh09, "removeHiddenElems", "removeHiddenElems.09.svg.txt");
fx!(rh10, "removeHiddenElems", "removeHiddenElems.10.svg.txt");
fx!(rh11, "removeHiddenElems", "removeHiddenElems.11.svg.txt");
fx!(rh12, "removeHiddenElems", "removeHiddenElems.12.svg.txt");
fx!(rh13, "removeHiddenElems", "removeHiddenElems.13.svg.txt");
fx!(rh14, "removeHiddenElems", "removeHiddenElems.14.svg.txt");
fx!(rh15, "removeHiddenElems", "removeHiddenElems.15.svg.txt");
fx!(rh16, "removeHiddenElems", "removeHiddenElems.16.svg.txt");
fx!(rh17, "removeHiddenElems", "removeHiddenElems.17.svg.txt");
fx!(rh18, "removeHiddenElems", "removeHiddenElems.18.svg.txt");
fx!(rh19, "removeHiddenElems", "removeHiddenElems.19.svg.txt");
fx!(ret01, "removeEmptyText", "removeEmptyText.01.svg.txt");
fx!(ret02, "removeEmptyText", "removeEmptyText.02.svg.txt");
fx!(ret03, "removeEmptyText", "removeEmptyText.03.svg.txt");
fx!(csp01, "convertShapeToPath", "convertShapeToPath.01.svg.txt");
fx!(csp02, "convertShapeToPath", "convertShapeToPath.02.svg.txt");
fx!(csp03, "convertShapeToPath", "convertShapeToPath.03.svg.txt");
fx!(csp04, "convertShapeToPath", "convertShapeToPath.04.svg.txt");
fx!(csp05, "convertShapeToPath", "convertShapeToPath.05.svg.txt");
fx!(cec01, "convertEllipseToCircle", "convertEllipseToCircle.01.svg.txt");
fx!(meag01, "moveElemsAttrsToGroup", "moveElemsAttrsToGroup.01.svg.txt");
fx!(meag02, "moveElemsAttrsToGroup", "moveElemsAttrsToGroup.02.svg.txt");
fx!(meag03, "moveElemsAttrsToGroup", "moveElemsAttrsToGroup.03.svg.txt");
fx!(meag04, "moveElemsAttrsToGroup", "moveElemsAttrsToGroup.04.svg.txt");
fx!(meag06, "moveElemsAttrsToGroup", "moveElemsAttrsToGroup.06.svg.txt");
fx!(meag07, "moveElemsAttrsToGroup", "moveElemsAttrsToGroup.07.svg.txt");
fx!(meag08, "moveElemsAttrsToGroup", "moveElemsAttrsToGroup.08.svg.txt");
fx!(mgate01, "moveGroupAttrsToElems", "moveGroupAttrsToElems.01.svg.txt");
fx!(mgate02, "moveGroupAttrsToElems", "moveGroupAttrsToElems.02.svg.txt");
fx!(mgate03, "moveGroupAttrsToElems", "moveGroupAttrsToElems.03.svg.txt");
fx!(mgate04, "moveGroupAttrsToElems", "moveGroupAttrsToElems.04.svg.txt");
fx!(mgate05, "moveGroupAttrsToElems", "moveGroupAttrsToElems.05.svg.txt");
fx!(mgate06, "moveGroupAttrsToElems", "moveGroupAttrsToElems.06.svg.txt");
