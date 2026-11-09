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

// --- security regression tests (canary must never leak) ---

fn uniq_temp_root(label: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "svgo-inline-sec-{label}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn b64(data: &[u8]) -> String {
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn assert_no_leak(out: &str, svg: &str, canary: &[u8]) {
    assert_eq!(out, svg, "href outside the allowed set must be left unchanged");
    assert!(
        !out.contains("data:"),
        "output must not contain a data URI embedding the canary"
    );
    assert!(
        !out.contains(&b64(canary)),
        "canary text must not leak (base64 form found in output)"
    );
    // plain-text form must not appear either
    assert!(
        !out.contains(std::str::from_utf8(canary).unwrap()),
        "canary text must not leak (plain form found in output)"
    );
}

#[test]
fn does_not_leak_canary_via_traversal_or_absolute_path() {
    let canary: &[u8] = b"CANARY-SECRET-DO-NOT-LEAK-7f3a91";
    let root = uniq_temp_root("trav");
    let base = root.join("input");
    std::fs::create_dir_all(&base).unwrap();
    // Canary lives OUTSIDE base_dir (sibling of input/), as a .txt and as a .png
    // so the check is about location, not just extension.
    let secret_txt = root.join("canary-secret.txt");
    let secret_png = root.join("canary-secret.png");
    std::fs::write(&secret_txt, canary).unwrap();
    std::fs::write(&secret_png, canary).unwrap();

    let hrefs: Vec<String> = vec![
        "../canary-secret.txt".to_string(),
        "../canary-secret.png".to_string(),
        "../../canary-secret.txt".to_string(),
        "subdir/../../canary-secret.txt".to_string(),
        secret_txt.to_string_lossy().into_owned(),
        secret_png.to_string_lossy().into_owned(),
        format!("/{}", secret_txt.display()),
        "..%2fcanary-secret.txt".to_string(),
        "%2e%2e%2fcanary-secret.txt".to_string(),
        "%2e%2e/canary-secret.png".to_string(),
    ];
    for href in &hrefs {
        let svg = format!(r#"<svg><image href="{}"/></svg>"#, href);
        let out = inline_images(&svg, &base);
        assert_no_leak(&out, &svg, canary);
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn does_not_leak_canary_via_symlink_inside_base() {
    #[cfg(unix)]
    {
        let canary: &[u8] = b"CANARY-SYMLINK-DO-NOT-LEAK-91bc02";
        let root = uniq_temp_root("symlink");
        let base = root.join("input");
        std::fs::create_dir_all(&base).unwrap();
        let secret = root.join("outside-secret.png");
        std::fs::write(&secret, canary).unwrap();
        std::os::unix::fs::symlink(&secret, base.join("link.png")).unwrap();
        let svg = r#"<svg><image href="link.png"/></svg>"#.to_string();
        let out = inline_images(&svg, &base);
        assert_no_leak(&out, &svg, canary);
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[test]
fn does_not_inline_non_image_file_inside_base() {
    let canary: &[u8] = b"CANARY-ENV-DO-NOT-LEAK-a1b2c3";
    let root = uniq_temp_root("nonimage");
    let base = root.join("input");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(base.join(".env"), canary).unwrap();
    std::fs::write(base.join("secret.txt"), canary).unwrap();
    for href in [".env", "secret.txt"] {
        let svg = format!(r#"<svg><image href="{}"/></svg>"#, href);
        let out = inline_images(&svg, &base);
        assert_no_leak(&out, &svg, canary);
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn does_not_inline_oversized_image() {
    let root = uniq_temp_root("oversized");
    let base = root.join("input");
    std::fs::create_dir_all(&base).unwrap();
    // 10 MiB limit in the fix + 1 byte; prefix is the canary so a leak is detectable.
    let canary: &[u8] = b"CANARY-BIG-DO-NOT-LEAK-ff0011";
    let mut data = Vec::with_capacity(10 * 1024 * 1024 + 1);
    data.extend_from_slice(canary);
    data.resize(10 * 1024 * 1024 + 1, b'A');
    std::fs::write(base.join("big.png"), &data).unwrap();
    let svg = r#"<svg><image href="big.png"/></svg>"#.to_string();
    let out = inline_images(&svg, &base);
    assert!(
        !out.contains("data:"),
        "oversized image must not be inlined"
    );
    assert_eq!(out, svg);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn still_inlines_safe_local_images() {
    let root = uniq_temp_root("safe");
    let base = root.join("input");
    std::fs::create_dir_all(base.join("images")).unwrap();
    std::fs::write(base.join("dot.png"), b"\x89PNG").unwrap();
    std::fs::write(base.join("images").join("photo.jpg"), b"\xff\xd8\xff").unwrap();
    let out1 = inline_images(r#"<svg><image href="dot.png"/></svg>"#, &base);
    assert_eq!(
        out1,
        r#"<svg><image href="data:image/png;base64,iVBORw=="/></svg>"#
    );
    let out2 = inline_images(r#"<svg><image href="./dot.png"/></svg>"#, &base);
    assert_eq!(
        out2,
        r#"<svg><image href="data:image/png;base64,iVBORw=="/></svg>"#
    );
    let out3 = inline_images(r#"<svg><image href="images/photo.jpg"/></svg>"#, &base);
    assert!(
        out3.contains("data:image/jpeg;base64,"),
        "nested safe image must still inline, got: {out3}"
    );
    let out4 = inline_images(r#"<svg><image href="images/../dot.png"/></svg>"#, &base);
    assert_eq!(
        out4,
        r#"<svg><image href="data:image/png;base64,iVBORw=="/></svg>"#
    );
    let _ = std::fs::remove_dir_all(&root);
}
