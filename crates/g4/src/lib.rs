pub mod collections;
pub mod parser;
pub mod path_data;
pub mod plugins;
pub mod stringifier;
pub mod style;
pub mod tools;
pub mod xast;

pub use parser::parse_svg;
pub use stringifier::stringify_svg;

fn param_bool(params: &str, key: &str) -> Option<bool> {
    let pat = format!("\"{}\"", key);
    let idx = params.find(&pat)?;
    let rest = params[idx + pat.len()..].trim_start_matches(|c: char| c.is_whitespace() || c == ':');
    if rest.starts_with("true") { Some(true) } else if rest.starts_with("false") { Some(false) } else { None }
}

fn param_num(params: &str, key: &str) -> Option<f64> {
    let pat = format!("\"{}\"", key);
    let idx = params.find(&pat)?;
    let rest = params[idx + pat.len()..].trim_start_matches(|c: char| c.is_whitespace() || c == ':');
    let mut end = 0;
    for (i, c) in rest.char_indices() {
        if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E' { end = i + c.len_utf8(); } else { break; }
    }
    rest[..end].parse::<f64>().ok()
}

/// Parse, apply a single named plugin with JSON params, and serialize
/// with the equivalent of SVGO `js2svg: { pretty: true }`.
pub fn optimize_with_plugin(input: &str, name: &str, params: &str) -> Result<String, String> {
    use plugins::Plugin;
    let mut root = parse_svg(input);
    match name {
        "convertEllipseToCircle" => plugins::convert_ellipse_to_circle::ConvertEllipseToCircle.apply(&mut root),
        "moveElemsAttrsToGroup" => plugins::move_elems_attrs_to_group::MoveElemsAttrsToGroup.apply(&mut root),
        "moveGroupAttrsToElems" => plugins::move_group_attrs_to_elems::MoveGroupAttrsToElems.apply(&mut root),
        "removeEmptyText" => {
            let d = plugins::remove_empty_text::RemoveEmptyTextParams::default();
            let p = plugins::remove_empty_text::RemoveEmptyTextParams {
                text: param_bool(params, "text").unwrap_or(d.text),
                tspan: param_bool(params, "tspan").unwrap_or(d.tspan),
                tref: param_bool(params, "tref").unwrap_or(d.tref),
            };
            plugins::remove_empty_text::RemoveEmptyText { params: p }.apply(&mut root);
        }
        "convertShapeToPath" => {
            let p = plugins::convert_shape_to_path::ConvertShapeToPathParams {
                convert_arcs: param_bool(params, "convertArcs").unwrap_or(false),
                float_precision: param_num(params, "floatPrecision").map(|v| v as i32),
            };
            plugins::convert_shape_to_path::ConvertShapeToPath { params: p }.apply(&mut root);
        }
        "removeHiddenElems" => {
            let d = plugins::remove_hidden_elems::RemoveHiddenElemsParams::default();
            let p = plugins::remove_hidden_elems::RemoveHiddenElemsParams {
                is_hidden: param_bool(params, "isHidden").unwrap_or(d.is_hidden),
                display_none: param_bool(params, "displayNone").unwrap_or(d.display_none),
                opacity0: param_bool(params, "opacity0").unwrap_or(d.opacity0),
                circle_r0: param_bool(params, "circleR0").unwrap_or(d.circle_r0),
                ellipse_rx0: param_bool(params, "ellipseRX0").unwrap_or(d.ellipse_rx0),
                ellipse_ry0: param_bool(params, "ellipseRY0").unwrap_or(d.ellipse_ry0),
                rect_width0: param_bool(params, "rectWidth0").unwrap_or(d.rect_width0),
                rect_height0: param_bool(params, "rectHeight0").unwrap_or(d.rect_height0),
                pattern_width0: param_bool(params, "patternWidth0").unwrap_or(d.pattern_width0),
                pattern_height0: param_bool(params, "patternHeight0").unwrap_or(d.pattern_height0),
                image_width0: param_bool(params, "imageWidth0").unwrap_or(d.image_width0),
                image_height0: param_bool(params, "imageHeight0").unwrap_or(d.image_height0),
                path_empty_d: param_bool(params, "pathEmptyD").unwrap_or(d.path_empty_d),
                polyline_empty_points: param_bool(params, "polylineEmptyPoints").unwrap_or(d.polyline_empty_points),
                polygon_empty_points: param_bool(params, "polygonEmptyPoints").unwrap_or(d.polygon_empty_points),
            };
            plugins::remove_hidden_elems::RemoveHiddenElems { params: p }.apply(&mut root);
        }
        other => return Err(format!("unknown plugin {other}")),
    }
    Ok(stringify_svg(&root))
}
