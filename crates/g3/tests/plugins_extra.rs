//! Additional param-variant tests derived from the JS plugin sources.

use svgo_g3::{optimize_with_plugin, Params};

fn run(input: &str, plugin: &str, params_json: &str) -> String {
    let params = Params::parse_json(params_json).unwrap();
    optimize_with_plugin(input, plugin, &params).unwrap().trim().to_string()
}

#[test]
fn numeric_params_variant() {
    let out = run(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="0.567px" height="1in"/>"#,
        "cleanupNumericValues",
        r#"{ "floatPrecision": 1, "leadingZero": false, "convertToPx": false }"#,
    );
    assert_eq!(out, r#"<svg xmlns="http://www.w3.org/2000/svg" width="0.6" height="1in"/>"#);
}

#[test]
fn colors_current_color_string_and_fractional_rgb() {
    let out = run(
        r#"<svg xmlns="http://www.w3.org/2000/svg"><path fill="red"/><path fill="blue"/></svg>"#,
        "convertColors",
        r#"{ "currentColor": "red" }"#,
    );
    assert_eq!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\">\n    <path fill=\"currentColor\"/>\n    <path fill=\"#00f\"/>\n</svg>"
    );
    let out = run(
        r#"<svg xmlns="http://www.w3.org/2000/svg"><path fill="rgb(10.5,20,30)"/></svg>"#,
        "convertColors",
        "{}",
    );
    assert!(out.contains(r##"fill="#0a141e""##), "{out}");
}

#[test]
fn colors_shorthex_requires_all_pairs_doubled() {
    // JS regHEX /^#(([a-fA-F0-9])\2){3}$/ only shortens when every pair is
    // doubled, so #aabbcd must stay #aabbcd (it is not #aabbcc).
    let out = run(
        r##"<svg xmlns="http://www.w3.org/2000/svg"><path fill="#aabbcd"/></svg>"##,
        "convertColors",
        "{}",
    );
    assert_eq!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\">\n    <path fill=\"#aabbcd\"/>\n</svg>"
    );
    // Same color via rgb(): rgb(170, 187, 205) is #aabbcd first.
    let out = run(
        r#"<svg xmlns="http://www.w3.org/2000/svg"><path fill="rgb(170, 187, 205)"/></svg>"#,
        "convertColors",
        "{}",
    );
    assert_eq!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\">\n    <path fill=\"#aabbcd\"/>\n</svg>"
    );
    // Uppercase input is lowercased first, then the same rule applies.
    let out = run(
        r##"<svg xmlns="http://www.w3.org/2000/svg"><path fill="#AABBCD"/></svg>"##,
        "convertColors",
        "{}",
    );
    assert_eq!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\">\n    <path fill=\"#aabbcd\"/>\n</svg>"
    );
    // Doubled first pairs must not rescue a differing third pair.
    let out = run(
        r##"<svg xmlns="http://www.w3.org/2000/svg"><path fill="#112234"/></svg>"##,
        "convertColors",
        "{}",
    );
    assert_eq!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\">\n    <path fill=\"#112234\"/>\n</svg>"
    );
    // Positive control: all pairs doubled still shortens.
    let out = run(
        r##"<svg xmlns="http://www.w3.org/2000/svg"><path fill="#aabbcc"/></svg>"##,
        "convertColors",
        "{}",
    );
    assert_eq!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\">\n    <path fill=\"#abc\"/>\n</svg>"
    );
}

#[test]
fn enable_background_style_variants() {
    let out = run(
        r#"<svg xmlns="http://www.w3.org/2000/svg" style="enable-background:new 0 0 10 10;color:red"><circle r="1"/></svg>"#,
        "cleanupEnableBackground",
        "{}",
    );
    assert!(out.contains(r#"style="color:red""#), "{out}");
    let out = run(
        r#"<svg xmlns="http://www.w3.org/2000/svg"><filter id="f"/><mask width="10" height="10" style="enable-background:new 0 0 10 10;fill:red"/></svg>"#,
        "cleanupEnableBackground",
        "{}",
    );
    assert!(out.contains(r#"style="enable-background:new;fill:red""#), "{out}");
}

#[test]
fn useless_stroke_from_inline_style() {
    let out = run(
        r#"<svg xmlns="http://www.w3.org/2000/svg"><circle style="stroke:none" stroke-width="5" fill="red"/></svg>"#,
        "removeUselessStrokeAndFill",
        "{}",
    );
    assert_eq!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\">\n    <circle style=\"stroke:none\" fill=\"red\"/>\n</svg>"
    );
}
