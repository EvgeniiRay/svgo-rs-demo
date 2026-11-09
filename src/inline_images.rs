//! `--inline-images`: replace `<image href="photo.png">` references with data URIs,
//! so an optimized SVG that uses local raster images works on its own.

use std::io::Read;
use std::path::Path;

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Maximum size of a file that may be inlined (10 MiB). Larger files are left as-is.
const MAX_INLINE_BYTES: u64 = 10 * 1024 * 1024;

const ALLOWED_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "svg", "avif", "bmp", "ico", "tif", "tiff", "apng",
];

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

fn mime_for_ext(ext: &str) -> &'static str {
    match ext {
        "png" | "apng" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "tif" | "tiff" => "image/tiff",
        _ => "application/octet-stream",
    }
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let hi = hex_val(bytes[i + 1])?;
            let lo = hex_val(bytes[i + 2])?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Resolve `href` against `base_dir` and read it only if it is a safe local image:
/// a relative path whose canonical location stays inside the canonical `base_dir`,
/// with an allowed image extension and a bounded size. Returns `None` (leave the
/// tag unchanged) for absolute paths, traversals, symlink escapes, schemes,
/// non-image files, missing files, and oversized files.
fn read_contained_image(base_dir: &Path, href: &str) -> Option<(Vec<u8>, &'static str)> {
    let trimmed = href.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("data:") || trimmed.contains("://") {
        return None;
    }
    // Strip query / fragment for the filesystem lookup.
    let path_part = trimmed.split(['?', '#']).next()?;
    if path_part.is_empty() {
        return None;
    }
    let decoded = percent_decode(path_part)?;
    if decoded.is_empty() || decoded.contains('\0') {
        return None;
    }
    // Treat backslash as a separator everywhere (it is one on Windows),
    // so `..\` traversals are checked the same way as `../`.
    let decoded = decoded.replace('\\', "/");
    // Any scheme (`file:`, `javascript:`, `C:`, ...) has ':' before the first '/'.
    if let Some(colon) = decoded.find(':') {
        let slash = decoded.find('/').unwrap_or(usize::MAX);
        if colon < slash {
            return None;
        }
    }
    // Absolute and protocol-relative paths must not discard `base_dir`.
    if decoded.starts_with('/') {
        return None;
    }
    let rel = Path::new(&decoded);
    if rel.is_absolute() {
        return None;
    }
    let ext = rel.extension()?.to_str()?.to_ascii_lowercase();
    if !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
        return None;
    }
    let effective_base: &Path = if base_dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        base_dir
    };
    let base_canonical = effective_base.canonicalize().ok()?;
    let candidate = base_canonical.join(rel);
    // Canonicalize resolves `..`, `.` and symlinks for both sides.
    let candidate_canonical = candidate.canonicalize().ok()?;
    if !candidate_canonical.starts_with(&base_canonical) {
        return None;
    }
    let metadata = std::fs::metadata(&candidate_canonical).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_INLINE_BYTES {
        return None;
    }
    let file = std::fs::File::open(&candidate_canonical).ok()?;
    let mut buf = Vec::new();
    // Cap the read as well, in case the file grows after the metadata check.
    file.take(MAX_INLINE_BYTES + 1).read_to_end(&mut buf).ok()?;
    if buf.len() as u64 > MAX_INLINE_BYTES {
        return None;
    }
    Some((buf, mime_for_ext(&ext)))
}

/// Inline every local `href` / `xlink:href` of an `<image>` element. Paths are relative to
/// `base_dir`, the folder of the input SVG. Only files inside `base_dir` with an image
/// extension and within the size limit are inlined. Remote URLs, existing data URIs,
/// absolute/traversal/symlink-escaping paths, non-image files and oversized files
/// stay as they are.
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
        if href.trim_start().to_ascii_lowercase().starts_with("data:") || href.contains("://") {
            return tag.to_string();
        }
        return match read_contained_image(base_dir, href) {
            Some((bytes, mime)) => format!("{}data:{};base64,{}{}", &tag[..v0], mime, base64(&bytes), &tag[v0 + len..]),
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
