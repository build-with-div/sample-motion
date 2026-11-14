//! The JavaScript number semantics Motion's formulas depend on.
//!
//! Motion's WAAPI output is a string built with `Math.round` and JavaScript's
//! number-to-string conversion, which differ from Rust's `f64::round` and
//! `Display` at the edges (half-way values, negative zero, exponents).

/// `Math.round`: rounds half-way values toward positive infinity, and keeps
/// the sign of zero (`Math.round(-0.4)` is `-0`).
pub fn math_round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let floor = x.floor();
    let rounded = if x - floor >= 0.5 { floor + 1.0 } else { floor };
    if rounded == 0.0 && x.is_sign_negative() {
        -0.0
    } else {
        rounded
    }
}

/// `Number.prototype.toString()` for finite and non-finite numbers.
///
/// Both JavaScript and Rust print the shortest digits that round-trip; they
/// differ in where exponents start, in the exponent's sign, and in `-0`.
pub fn number_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if x == 0.0 {
        return "0".into();
    }
    let magnitude = x.abs();
    if (1e-6..1e21).contains(&magnitude) {
        return format!("{x}");
    }
    // Rust prints `1e-7` and `1e21`; JavaScript prints `1e-7` and `1e+21`.
    let exponential = format!("{x:e}");
    match exponential.split_once('e') {
        Some((mantissa, exponent)) if !exponent.starts_with('-') => {
            format!("{mantissa}e+{exponent}")
        }
        _ => exponential,
    }
}

/// motion-utils `clamp(min, max, v)`.
pub fn clamp(min: f64, max: f64, value: f64) -> f64 {
    if value > max {
        max
    } else if value < min {
        min
    } else {
        value
    }
}

pub fn seconds_to_milliseconds(seconds: f64) -> f64 {
    seconds * 1000.0
}

pub fn milliseconds_to_seconds(milliseconds: f64) -> f64 {
    milliseconds / 1000.0
}

/// motion-utils `progress(from, to, value)`.
pub fn progress(from: f64, to: f64, value: f64) -> f64 {
    let difference = to - from;
    if difference == 0.0 {
        1.0
    } else {
        (value - from) / difference
    }
}

/// motion-dom `mixNumber(from, to, progress)`.
pub fn mix_number(from: f64, to: f64, progress: f64) -> f64 {
    from + (to - from) * progress
}

/// motion-utils `wrap(min, max, v)`.
pub fn wrap(min: f64, max: f64, value: f64) -> f64 {
    let range = max - min;
    (((value - min) % range) + range) % range + min
}

/// `Math.hypot` as V8 computes it: scaled by the largest magnitude and
/// summed with Kahan compensation, so `hypot(a, 0)` is exactly `|a|`.
pub fn hypot(values: &[f64]) -> f64 {
    let mut max: f64 = 0.0;
    let mut any_nan = false;
    for value in values {
        let magnitude = value.abs();
        if magnitude.is_infinite() {
            return f64::INFINITY;
        }
        any_nan |= magnitude.is_nan();
        if magnitude > max {
            max = magnitude;
        }
    }
    if any_nan {
        return f64::NAN;
    }
    if max == 0.0 {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut compensation = 0.0;
    for value in values {
        let n = value.abs() / max;
        let summand = n * n - compensation;
        let preliminary = sum + summand;
        compensation = (preliminary - sum) - summand;
        sum = preliminary;
    }
    sum.sqrt() * max
}

/// `parseFloat(text)`: the longest decimal literal at the start of the
/// string (after whitespace), or NaN.
pub fn parse_float(text: &str) -> f64 {
    let text = text.trim_start();
    let bytes = text.as_bytes();
    let mut end = 0;
    if matches!(bytes.first(), Some(b'+' | b'-')) {
        end = 1;
    }
    if text[end..].starts_with("Infinity") {
        return if text.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let digits_start = end;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    let mut digits = end - digits_start;
    if end < bytes.len() && bytes[end] == b'.' {
        let fraction_start = end + 1;
        let mut fraction_end = fraction_start;
        while fraction_end < bytes.len() && bytes[fraction_end].is_ascii_digit() {
            fraction_end += 1;
        }
        digits += fraction_end - fraction_start;
        if digits > 0 {
            end = fraction_end;
        }
    }
    if digits == 0 {
        return f64::NAN;
    }
    if end < bytes.len() && matches!(bytes[end], b'e' | b'E') {
        let mut exponent_end = end + 1;
        if matches!(bytes.get(exponent_end), Some(b'+' | b'-')) {
            exponent_end += 1;
        }
        let exponent_digits = exponent_end;
        while exponent_end < bytes.len() && bytes[exponent_end].is_ascii_digit() {
            exponent_end += 1;
        }
        if exponent_end > exponent_digits {
            end = exponent_end;
        }
    }
    text[..end].parse().unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn math_round_matches_javascript() {
        assert_eq!(math_round(0.5), 1.0);
        assert_eq!(math_round(-0.5).to_bits(), (-0.0_f64).to_bits());
        assert_eq!(math_round(-1.5), -1.0);
        assert_eq!(math_round(2.4999), 2.0);
        assert_eq!(math_round(-0.4).to_bits(), (-0.0_f64).to_bits());
    }

    #[test]
    fn number_to_string_matches_javascript() {
        assert_eq!(number_to_string(-0.0), "0");
        assert_eq!(number_to_string(1.0), "1");
        assert_eq!(number_to_string(0.1234), "0.1234");
        assert_eq!(number_to_string(-1.25), "-1.25");
        assert_eq!(number_to_string(1e-7), "1e-7");
        assert_eq!(number_to_string(0.000001), "0.000001");
        assert_eq!(number_to_string(1e21), "1e+21");
        assert_eq!(number_to_string(123456789012.5), "123456789012.5");
    }

    #[test]
    fn parse_float_matches_javascript() {
        assert_eq!(parse_float("+0.5"), 0.5);
        assert_eq!(parse_float("-0.2"), -0.2);
        assert_eq!(parse_float("0.15abc"), 0.15);
        assert_eq!(parse_float(".5"), 0.5);
        assert_eq!(parse_float("1e3x"), 1000.0);
        assert_eq!(parse_float("1e"), 1.0);
        assert!(parse_float("x").is_nan());
        assert!(parse_float("").is_nan());
        assert!(parse_float(".").is_nan());
    }

    #[test]
    fn hypot_matches_v8() {
        assert_eq!(hypot(&[-1.0, 0.0]), 1.0);
        assert_eq!(hypot(&[3.0, 4.0]), 5.0);
        assert_eq!(hypot(&[0.0, 0.0]), 0.0);
        assert_eq!(hypot(&[2.0, -0.5]), 2.0 * (1.0_f64 + 0.0625).sqrt());
    }
}
