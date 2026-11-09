//! `--inline-images`: replace `<image href="photo.png">` references with data URIs,
//! so an optimized SVG that uses local raster images works on its own.

use std::path::Path;

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

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
        return match std::fs::read(base_dir.join(href)) {
            Ok(bytes) => format!("{}data:{};base64,{}{}", &tag[..v0], mime(href), base64(&bytes), &tag[v0 + len..]),
            Err(_) => tag.to_string(),
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
