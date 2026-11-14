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
}
