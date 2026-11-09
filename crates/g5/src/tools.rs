// Shared numeric helpers (port of lib/svgo/tools.js).
// SHARED-HELPER note: see submit unresolved list.

/// JS `toFixed` helper from lib/svgo/tools.js: `Math.round(num * pow) / pow`.
/// Note this is NOT `Number.prototype.toFixed` (which is decimal-exact);
/// SVGO's helper rounds half-up via Math.round (floor(x + 0.5)).
pub fn to_fixed(num: f64, precision: i32) -> f64 {
    let pow = 10f64.powi(precision);
    (num * pow + 0.5).floor() / pow
}

/// JS `Math.round`.
pub fn js_round(num: f64) -> f64 {
    (num + 0.5).floor()
}

/// JS number -> string (shortest form, `0.5`, `10`, `-0` -> `0`).
pub fn fmt_num(value: f64) -> String {
    let mut s = String::new();
    fmt_num_into(&mut s, value);
    s
}

fn push_u64(out: &mut String, mut n: u64) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    for &b in &buf[i..] {
        out.push(b as char);
    }
}

fn push_i64(out: &mut String, n: i64) {
    if n < 0 {
        out.push('-');
        push_u64(out, n.unsigned_abs());
    } else {
        push_u64(out, n as u64);
    }
}

/// Append the `fmt_num` form of `value` to `out` without formatting
/// through `core::fmt` in the common case.
///
/// Most values reaching this helper have just been rounded by
/// `to_fixed` to a few decimal places. For those, the shortest string
/// is the rounded decimal itself: if `k / 10^p` (with `p` minimal)
/// reproduces `value` exactly and `k < 2^51`, then `10^-p` is wider
/// than one ulp of `value`, so no shorter decimal can round-trip to
/// the same float. That decimal is written with integer arithmetic;
/// anything else falls back to the standard float formatting.
fn fmt_num_into(out: &mut String, value: f64) {
    if value == 0.0 {
        out.push('0');
        return;
    }
    if !value.is_finite() {
        out.push_str(&value.to_string());
        return;
    }
    let av = value.abs();
    if av >= 1e21 || av < 1e-6 {
        // JS exponential: 1e+21, 1.5e-7
        let s = format!("{:e}", value);
        if let Some((m, e)) = s.split_once('e') {
            let exp: i64 = e.parse().unwrap_or(0);
            out.push_str(m);
            out.push('e');
            out.push(if exp >= 0 { '+' } else { '-' });
            push_u64(out, exp.unsigned_abs());
        } else {
            out.push_str(&s);
        }
        return;
    }
    if value == value.trunc() && av < 1e15 {
        push_i64(out, value as i64);
        return;
    }
    // Fast path for short decimals (see above). `scaled < 2^51`
    // keeps `10^-p` wider than one ulp, proving the trimmed decimal
    // is the shortest round-trip form the fallback would produce.
    const MAX_SCALED: f64 = 2_251_799_813_685_248.0; // 2^51
    for p in 1..=15u32 {
        let pow = 10f64.powi(p as i32);
        let scaled = (av * pow).round();
        if scaled >= MAX_SCALED {
            break;
        }
        if scaled / pow != av {
            continue;
        }
        let scaled = scaled as u64;
        if value < 0.0 {
            out.push('-');
        }
        // Write `scaled` with the point `p` digits from the right,
        // padding with leading zeros for values below 1.
        let mut buf = [0u8; 20];
        let mut n = scaled;
        let mut i = buf.len();
        loop {
            i -= 1;
            buf[i] = b'0' + (n % 10) as u8;
            n /= 10;
            if n == 0 {
                break;
            }
        }
        let digits = &buf[i..];
        let p = p as usize;
        if digits.len() > p {
            let split = digits.len() - p;
            for &b in &digits[..split] {
                out.push(b as char);
            }
            out.push('.');
            for &b in &digits[split..] {
                out.push(b as char);
            }
        } else {
            out.push_str("0.");
            for _ in digits.len()..p {
                out.push('0');
            }
            for &b in digits {
                out.push(b as char);
            }
        }
        // Trailing zeros cannot occur for the minimal `p`, but trim
        // anyway: dropping them does not change the parsed value.
        while out.ends_with('0') {
            out.pop();
        }
        if out.ends_with('.') {
            out.pop();
        }
        return;
    }
    out.push_str(&format!("{}", value));
}

fn remove_leading_zero_into(out: &mut String, value: f64) {
    let start = out.len();
    fmt_num_into(out, value);
    if value > 0.0 && value < 1.0 && out.as_bytes().get(start) == Some(&b'0') {
        out.remove(start);
    } else if value > -1.0
        && value < 0.0
        && out.len() - start > 1
        && out.as_bytes()[start + 1] == b'0'
    {
        out.remove(start + 1);
    }
}

/// Port of `removeLeadingZero`.
pub fn remove_leading_zero(value: f64) -> String {
    let mut s = String::new();
    remove_leading_zero_into(&mut s, value);
    s
}

#[cfg(test)]
mod tests {
    use super::{fmt_num, remove_leading_zero};

    #[test]
    fn fmt_num_matches_shortest_form() {
        let cases = [
            (0.0, "0"),
            (-0.0, "0"),
            (0.5, "0.5"),
            (-0.5, "-0.5"),
            (12.75, "12.75"),
            (20.4142, "20.4142"),
            (0.000001, "0.000001"),
            (0.0000015, "0.0000015"),
            (1e21, "1e+21"),
            (1.5e-7, "1.5e-7"),
            (0.30000000000000004, "0.30000000000000004"),
            (123456789.123456, "123456789.123456"),
        ];
        for (value, expected) in cases {
            assert_eq!(fmt_num(value), expected, "fmt_num({value})");
        }
        assert_eq!(remove_leading_zero(0.5), ".5");
        assert_eq!(remove_leading_zero(-0.5), "-.5");
        assert_eq!(remove_leading_zero(12.75), "12.75");
    }
}

pub struct CleanupParams {
    pub leading_zero: bool,
    pub negative_extra_space: bool,
    pub no_space_after_flags: bool,
}

/// Port of `cleanupOutData` (lib/svgo/tools.js).
pub fn cleanup_out_data(data: &[f64], params: &CleanupParams, command: Option<char>) -> String {
    let mut str_out = String::new();
    let mut item_str = String::new();
    let mut prev: Option<f64> = None;
    for (i, item) in data.iter().enumerate() {
        let mut delimiter = " ";
        if i == 0 {
            delimiter = "";
        }
        if params.no_space_after_flags && (command == Some('A') || command == Some('a')) {
            let pos = i % 7;
            if pos == 4 || pos == 5 {
                delimiter = "";
            }
        }
        item_str.clear();
        if params.leading_zero {
            remove_leading_zero_into(&mut item_str, *item);
        } else {
            fmt_num_into(&mut item_str, *item);
        }
        if params.negative_extra_space
            && !delimiter.is_empty()
            && (*item < 0.0
                || (item_str.starts_with('.') && prev.map(|p| p % 1.0 != 0.0).unwrap_or(false)))
        {
            delimiter = "";
        }
        prev = Some(*item);
        str_out.push_str(delimiter);
        str_out.push_str(&item_str);
    }
    str_out
}

pub fn includes_url_reference(body: &str) -> bool {
    // \burl\(("')?#(.+?)\1\)
    body.contains("url(#") || body.contains("url('#") || body.contains("url(\"#")
}

/// Extract `#id` references like findReferences for url(...) and href="#...".
pub fn find_references(attr: &str, value: &str) -> Vec<String> {
    let mut out = Vec::new();
    const REF_PROPS: &[&str] = &[
        "fill", "stroke", "filter", "clip-path", "mask", "marker", "marker-start",
        "marker-mid", "marker-end",
    ];
    if REF_PROPS.contains(&attr) {
        let mut rest = value;
        while let Some(idx) = rest.find("url(") {
            let after = &rest[idx + 4..];
            let after = after.trim_start_matches(|c| c == '"' || c == '\'');
            if let Some(hash) = after.strip_prefix('#') {
                let id: String = hash.chars().take_while(|c| *c != ')' && *c != '"' && *c != '\'').collect();
                out.push(id);
            }
            rest = &rest[idx + 4..];
        }
    }
    if attr == "href" || attr.ends_with(":href") {
        if let Some(id) = value.strip_prefix('#') {
            out.push(id.to_string());
        }
    }
    if attr == "begin" {
        if let Some(dot) = value.find('.') {
            out.push(value[..dot].to_string());
        }
    }
    out
}
