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
fn canary_outside_base_dir_cannot_leak_via_traversal_or_absolute_path() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("svgo-canary-{}-{nanos}", std::process::id()));
    let base = root.join("project");
    std::fs::create_dir_all(&base).unwrap();
    // Canary is an image-extension file outside base_dir: only containment
    // (not an extension check) can keep it from being embedded.
    let canary = b"CANARY-DO-NOT-LEAK-7f3a9c";
    std::fs::write(root.join("canary.png"), canary).unwrap();

    let traversal_svg = r#"<svg><image href="../canary.png"/></svg>"#;
    let traversal_out = inline_images(traversal_svg, &base);

    let absolute_href = root.join("canary.png").display().to_string();
    let absolute_svg = format!(r#"<svg><image href="{absolute_href}"/></svg>"#);
    let absolute_out = inline_images(&absolute_svg, &base);

    std::fs::remove_dir_all(&root).unwrap();

    // Base64 of the canary must not appear, and the href must be left as-is.
    assert_eq!(traversal_out, traversal_svg, "traversal href leaked or was rewritten");
    assert!(!traversal_out.contains("data:"), "traversal leaked canary: {traversal_out}");
    assert_eq!(absolute_out, absolute_svg, "absolute href leaked or was rewritten");
    assert!(!absolute_out.contains("data:"), "absolute path leaked canary: {absolute_out}");
}

#[test]
fn leaves_remote_and_data_urls_alone() {
    let svg = r#"<svg><image href="https://example.com/a.png"/><image href="data:image/png;base64,AA=="/></svg>"#;
    assert_eq!(inline_images(svg, Path::new(".")), svg);
}
