use std::path::Path;
use svgo_rs::inline_images::inline_images;

#[test]
fn inlines_a_local_image_next_to_the_svg() {
    let dir = std::env::temp_dir().join(format!("svgo-inline-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("dot.png"), b"\x89PNG").unwrap();
    let out = inline_images(r#"<svg><image href="dot.png" width="1" height="1"/></svg>"#, &dir);
    assert_eq!(out, r#"<svg><image href="data:image/png;base64,iVBORw==" width="1" height="1"/></svg>"#);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn leaves_remote_and_data_urls_alone() {
    let svg = r#"<svg><image href="https://example.com/a.png"/><image href="data:image/png;base64,AA=="/></svg>"#;
    assert_eq!(inline_images(svg, Path::new(".")), svg);
}
