//! svgo-rs: Rust port of SVGO 4.1.0 `preset-default`.
//!
//! The 34 preset plugins live in six group crates under `crates/`
//! (ported by parallel agents, each passing its upstream fixture
//! tests) plus four lead-ported plugins in `lead_plugins`. Plugins run
//! in preset order; each stage parses, applies one plugin, and
//! serializes, and the final output is serialized compactly, matching
//! SVGO's default `js2svg` settings (no pretty printing, no trailing
//! newline).

pub mod inline_images;
pub mod lead_plugins;

use lead_plugins::UnknownsParams;

/// The preset-default plugin list, in order (reference/svgo
/// `plugins/preset-default.js`, v4.1.0).
pub const PRESET_DEFAULT: [&str; 34] = [
    "removeDoctype",
    "removeXMLProcInst",
    "removeComments",
    "removeDeprecatedAttrs",
    "removeMetadata",
    "removeEditorsNSData",
    "cleanupAttrs",
    "mergeStyles",
    "inlineStyles",
    "minifyStyles",
    "cleanupIds",
    "removeUselessDefs",
    "cleanupNumericValues",
    "convertColors",
    "removeUnknownsAndDefaults",
    "removeNonInheritableGroupAttrs",
    "removeUselessStrokeAndFill",
    "cleanupEnableBackground",
    "removeHiddenElems",
    "removeEmptyText",
    "convertShapeToPath",
    "convertEllipseToCircle",
    "moveElemsAttrsToGroup",
    "moveGroupAttrsToElems",
    "collapseGroups",
    "convertPathData",
    "convertTransform",
    "removeEmptyAttrs",
    "removeEmptyContainers",
    "mergePaths",
    "removeUnusedNS",
    "sortAttrs",
    "sortDefsChildren",
    "removeDesc",
];

fn param_bool(params: &str, key: &str) -> Option<bool> {
    let pat = format!("\"{}\"", key);
    let idx = params.find(&pat)?;
    let rest = params[idx + pat.len()..]
        .trim_start_matches(|c: char| c.is_whitespace() || c == ':');
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn lead_plugin(input: &str, name: &str, params: &str) -> Result<String, String> {
    let mut root = g6::parser::parse_svg(input)?;
    match name {
        "removeDeprecatedAttrs" => {
            lead_plugins::remove_deprecated_attrs(
                &mut root,
                param_bool(params, "removeUnsafe").unwrap_or(false),
            );
        }
        "removeUnknownsAndDefaults" => {
            let d = UnknownsParams::default();
            let p = UnknownsParams {
                unknown_content: param_bool(params, "unknownContent").unwrap_or(d.unknown_content),
                unknown_attrs: param_bool(params, "unknownAttrs").unwrap_or(d.unknown_attrs),
                default_attrs: param_bool(params, "defaultAttrs").unwrap_or(d.default_attrs),
                default_markup_declarations: param_bool(params, "defaultMarkupDeclarations")
                    .unwrap_or(d.default_markup_declarations),
                useless_overrides: param_bool(params, "uselessOverrides").unwrap_or(d.useless_overrides),
                keep_data_attrs: param_bool(params, "keepDataAttrs").unwrap_or(d.keep_data_attrs),
                keep_aria_attrs: param_bool(params, "keepAriaAttrs").unwrap_or(d.keep_aria_attrs),
                keep_role_attr: param_bool(params, "keepRoleAttr").unwrap_or(d.keep_role_attr),
            };
            lead_plugins::remove_unknowns_and_defaults(&mut root, &p);
        }
        "sortAttrs" => {
            let order = extract_order(params);
            let xmlns_front = !params.contains("\"xmlnsOrder\"") || !params.contains("\"alphabetical\"");
            lead_plugins::sort_attrs(&mut root, order, xmlns_front);
        }
        "sortDefsChildren" => lead_plugins::sort_defs_children(&mut root),
        other => return Err(format!("unknown lead plugin {other}")),
    }
    Ok(g6::stringifier::stringify_svg(
        &root,
        &g6::stringifier::StringifyOptions { pretty: true, ..Default::default() },
    ))
}

fn extract_order(params: &str) -> Option<Vec<String>> {
    let idx = params.find("\"order\"")?;
    let rest = &params[idx..];
    let start = rest.find('[')?;
    let end = rest.find(']')?;
    let inner = &rest[start + 1..end];
    let out: Vec<String> = inner
        .split(',')
        .map(|s| s.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if out.is_empty() { None } else { Some(out) }
}

/// Run a single plugin by SVGO name with JSON params (`{}` for
/// defaults), serialized pretty like the upstream fixture harness.
pub fn optimize_with_plugin(input: &str, name: &str, params: &str) -> Result<String, String> {
    let params = if params.trim().is_empty() { "{}" } else { params };
    match name {
        "removeDoctype" | "removeXMLProcInst" | "removeComments" | "removeMetadata"
        | "removeEditorsNSData" | "cleanupAttrs" => {
            g1::optimize_with_plugin(input, name, params)
        }
        "mergeStyles" | "inlineStyles" | "minifyStyles" | "cleanupIds" | "removeUselessDefs" => {
            g2::optimize_plugin(input, name, params).ok_or_else(|| format!("unknown plugin {name}"))
        }
        "cleanupNumericValues" | "convertColors" | "removeNonInheritableGroupAttrs"
        | "removeUselessStrokeAndFill" | "cleanupEnableBackground" => {
            let p = g3::Params::parse_json(params)?;
            g3::optimize_with_plugin(input, name, &p)
        }
        "removeHiddenElems" | "removeEmptyText" | "convertShapeToPath" | "convertEllipseToCircle"
        | "moveElemsAttrsToGroup" | "moveGroupAttrsToElems" => {
            g4::optimize_with_plugin(input, name, params)
        }
        "collapseGroups" | "collapsibleGroups" | "convertPathData" | "convertTransform"
        | "removeEmptyAttrs" | "removeEmptyContainers" | "mergePaths" => {
            Ok(g5::optimize_with_plugin(input, name, &g5::json::parse_json(params)))
        }
        "removeUnusedNS" => g6::optimize_with_plugin(input, &g6::RemoveUnusedNs),
        "removeTitle" => g6::optimize_with_plugin(input, &g6::RemoveTitle),
        "removeDimensions" => g6::optimize_with_plugin(input, &g6::RemoveDimensions),
        "removeDesc" => {
            let p = g6::plugins::remove_desc::RemoveDescParams {
                remove_any: param_bool(params, "removeAny").unwrap_or(false),
            };
            g6::optimize_with_plugin(input, &g6::RemoveDesc(p))
        }
        "cleanupListOfValues" => {
            let d = g6::plugins::cleanup_list_of_values::CleanupListOfValuesParams::default();
            let p = g6::plugins::cleanup_list_of_values::CleanupListOfValuesParams {
                float_precision: param_bool(params, "__none").map(|_| d.float_precision).unwrap_or(d.float_precision),
                leading_zero: param_bool(params, "leadingZero").unwrap_or(d.leading_zero),
                default_px: param_bool(params, "defaultPx").unwrap_or(d.default_px),
                convert_to_px: param_bool(params, "convertToPx").unwrap_or(d.convert_to_px),
            };
            g6::optimize_with_plugin(input, &g6::CleanupListOfValues(p))
        }
        "removeDeprecatedAttrs" | "removeUnknownsAndDefaults" | "sortAttrs" | "sortDefsChildren" => {
            lead_plugin(input, name, params)
        }
        other => Err(format!("unknown plugin {other}")),
    }
}

/// Serialize with SVGO's default (compact) stringifier settings.
pub fn stringify_compact(input: &str) -> Result<String, String> {
    let root = g6::parser::parse_svg(input)?;
    Ok(g6::stringifier::stringify_svg(&root, &g6::stringifier::StringifyOptions::default()))
}

/// Optimize with the full preset-default (single pass), returning
/// compact output with no trailing newline, byte-comparable with
/// `optimize()` from JS SVGO with default settings.
pub fn optimize_preset(input: &str) -> Result<String, String> {
    let mut current = input.to_string();
    for name in PRESET_DEFAULT {
        current = optimize_with_plugin(&current, name, "{}")?;
    }
    stringify_compact(&current)
}
