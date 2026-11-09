//! `--inline-images`: replace `<image href="photo.png">` references with data URIs,
//! so an optimized SVG that uses local raster images works on its own.

use std::io::Read;
use std::path::Path;

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Largest image that will be inlined. Bounds memory use for untrusted SVGs.
const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;

fn base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
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

fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn is_allowed_image(href: &str) -> bool {
    matches!(
        href.rsplit('.').next().map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "svg")
    )
}

/// Read `href` only if it is a regular image file contained in `base_dir`.
/// Absolute paths, `../` escapes, and symlinks pointing outside are rejected
/// by canonicalizing both paths and requiring containment.
fn read_contained_image(base_dir: &Path, href: &str) -> Option<Vec<u8>> {
    if href.is_empty() || !is_allowed_image(href) {
        return None;
    }
    let href_path = Path::new(href);
    if href_path.is_absolute() {
        return None;
    }
    let base_ref = if base_dir.as_os_str().is_empty() { Path::new(".") } else { base_dir };
    let canonical_base = base_ref.canonicalize().ok()?;
    let canonical_target = canonical_base.join(href_path).canonicalize().ok()?;
    if !canonical_target.starts_with(&canonical_base) || !canonical_target.is_file() {
        return None;
    }
    let file = std::fs::File::open(&canonical_target).ok()?;
    if file.metadata().ok()?.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_IMAGE_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return None;
    }
    Some(bytes)
}

/// Inline every local `href` / `xlink:href` of an `<image>` element. Paths are relative to
/// `base_dir`, the folder of the input SVG. Remote URLs and existing data URIs stay as they are.
pub fn inline_images(svg: &str, base_dir: &Path) -> String {
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(start) = rest.find("<image") {
        let end = rest[start..].find('>').map(|e| start + e + 1).unwrap_or(rest.len());
        out.push_str(&rest[..start]);
        out.push_str(&inline_tag(&rest[start..end], base_dir));
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

fn inline_tag(tag: &str, base_dir: &Path) -> String {
    for attr in ["xlink:href=\"", "href=\""] {
        let Some(i) = tag.find(attr) else { continue };
        let v0 = i + attr.len();
        let Some(len) = tag[v0..].find('"') else { continue };
        let href = &tag[v0..v0 + len];
        if href.starts_with("data:") || href.contains("://") {
            return tag.to_string();
        }
        return match read_contained_image(base_dir, href) {
            Some(bytes) => format!("{}data:{};base64,{}{}", &tag[..v0], mime(href), base64(&bytes), &tag[v0 + len..]),
            None => tag.to_string(),
        };
    }
    tag.to_string()
}

#[cfg(test)]
mod tests {
    use super::base64;

    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
