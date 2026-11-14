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

/// `Math.exp` as V8 computes it: fdlibm's `__ieee754_exp` with V8's
/// special case for `exp(1)` (`src/base/ieee754.cc`). It can differ from
/// the platform's `exp` in the last bit.
pub fn math_exp(x: f64) -> f64 {
    // fdlibm's constants, by their bit patterns as its comments give them.
    const HALF: [f64; 2] = [0.5, -0.5];
    const O_THRESHOLD: f64 = f64::from_bits(0x4086_2E42_FEFA_39EF);
    const U_THRESHOLD: f64 = f64::from_bits(0xC087_4910_D52D_3051);
    const LN2_HI: [f64; 2] = [
        f64::from_bits(0x3FE6_2E42_FEE0_0000),
        f64::from_bits(0xBFE6_2E42_FEE0_0000),
    ];
    const LN2_LO: [f64; 2] = [
        f64::from_bits(0x3DEA_39EF_3579_3C76),
        f64::from_bits(0xBDEA_39EF_3579_3C76),
    ];
    const INV_LN2: f64 = f64::from_bits(0x3FF7_1547_652B_82FE);
    const P1: f64 = f64::from_bits(0x3FC5_5555_5555_553E);
    const P2: f64 = f64::from_bits(0xBF66_C16C_16BE_BD93);
    const P3: f64 = f64::from_bits(0x3F11_566A_AF25_DE2C);
    const P4: f64 = f64::from_bits(0xBEBB_BD41_C5D2_6BF1);
    const P5: f64 = f64::from_bits(0x3E66_3769_72BE_A4D0);
    const HUGE: f64 = 1.0e300;
    // 2^-1000
    const TWO_M1000: f64 = f64::from_bits(0x0170_0000_0000_0000);

    let high = (x.to_bits() >> 32) as u32;
    let sign = (high >> 31) as usize;
    let high = high & 0x7fff_ffff;
    // Non-finite and out-of-range arguments.
    if high >= 0x4086_2E42 {
        if high >= 0x7ff0_0000 {
            if ((high & 0xf_ffff) | x.to_bits() as u32) != 0 {
                return x + x;
            }
            return if sign == 0 { x } else { 0.0 };
        }
        if x > O_THRESHOLD {
            return HUGE * HUGE;
        }
        if x < U_THRESHOLD {
            return TWO_M1000 * TWO_M1000;
        }
    }
    // Argument reduction.
    let mut x = x;
    let (mut hi, mut lo, k): (f64, f64, i32) = (0.0, 0.0, 0);
    let k = if high > 0x3fd6_2e42 {
        let k = if high < 0x3FF0_A2B2 {
            // V8 returns E for exp(1), where fdlibm is one bit off.
            if x == 1.0 {
                return std::f64::consts::E;
            }
            hi = x - LN2_HI[sign];
            lo = LN2_LO[sign];
            1 - 2 * sign as i32
        } else {
            let k = (INV_LN2 * x + HALF[sign]) as i32;
            let t = k as f64;
            hi = x - t * LN2_HI[0];
            lo = t * LN2_LO[0];
            k
        };
        x = hi - lo;
        k
    } else if high < 0x3e30_0000 {
        if HUGE + x > 1.0 {
            return 1.0 + x;
        }
        k
    } else {
        k
    };
    // x is now in the primary range.
    let t = x * x;
    let c = x - t * (P1 + t * (P2 + t * (P3 + t * (P4 + t * P5))));
    if k == 0 {
        return 1.0 - ((x * c) / (c - 2.0) - x);
    }
    let y = 1.0 - ((lo - (x * c) / (2.0 - c)) - hi);
    if k >= -1021 {
        if k == 1024 {
            return y * 2.0 * f64::from_bits(0x7fe0_0000_0000_0000);
        }
        y * f64::from_bits(((0x3ff + k) as u64) << 52)
    } else {
        y * f64::from_bits(((0x3ff + k + 1000) as u64) << 52) * TWO_M1000
    }
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
    fn math_exp_matches_v8() {
        // `Math.exp(x)` printed by Node 22.18 (V8).
        let cases: [(f64, f64); 8] = [
            (1.0, std::f64::consts::E), // printed as 2.718281828459045
            (-1.0, 0.36787944117144233),
            (0.5, 1.6487212707001282),
            (-0.0014, 0.9986009795428267),
            (3.7, 40.4473043600674),
            (-20.0, 2.061153622438558e-9),
            (700.0, 1.0142320547350045e304),
            (-740.0, 4.2e-322),
        ];
        for (x, expected) in cases {
            assert_eq!(math_exp(x).to_bits(), expected.to_bits(), "exp({x})");
        }
        assert_eq!(math_exp(0.0), 1.0);
        assert_eq!(math_exp(f64::NEG_INFINITY), 0.0);
        assert_eq!(math_exp(f64::INFINITY), f64::INFINITY);
        assert_eq!(math_exp(710.0), f64::INFINITY);
        assert_eq!(math_exp(-746.0), 0.0);
        assert!(math_exp(f64::NAN).is_nan());
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
