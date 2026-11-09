//! Helpers mirroring `removeLeadingZero` / `Number#toFixed` /
//! `Number#toString` in `reference/svgo/lib/svgo/tools.js`.

/// JS `Number.prototype.toString()` for the magnitudes these plugins
/// produce. Rust's `Display` for `f64` is also shortest-round-trip, but
/// never uses exponent notation and prints integers without `.0`, which
/// matches JS below 1e21 and at/above 1e-6 after fixed-precision rounding.
pub fn js_number_to_string(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string(); // also covers -0, as in JS
    }
    if value.fract() == 0.0 && value.abs() < 1e21 {
        return format!("{value:.0}");
    }
    format!("{value}")
}

/// JS `Number(Number(x).toFixed(precision))`.
pub fn to_fixed_number(value: f64, precision: usize) -> f64 {
    // Rust float formatting rounds the exact binary value, like JS toFixed.
    format!("{value:.precision$}").parse::<f64>().unwrap_or(value)
}

/// Mirror of `removeLeadingZero` in tools.js.
pub fn remove_leading_zero(value: f64) -> String {
    let str_value = js_number_to_string(value);
    if value > 0.0 && value < 1.0 && str_value.starts_with('0') {
        return str_value[1..].to_string();
    }
    if value > -1.0 && value < 0.0 && str_value.as_bytes().get(1) == Some(&b'0') {
        return format!("{}{}", &str_value[..1], &str_value[2..]);
    }
    str_value
}
