//! The JavaScript values anime.js's parameters hold, and the parts of
//! `core/values.js`, `core/transforms.js` and `core/helpers.js` that read
//! and write them.

use super::ease::EaseSpec;
use super::stagger::Stagger;
use crate::motion::js::{math_round, number_to_string};
use std::collections::BTreeMap;
use std::rc::Rc;

/// A parameter value: what a JavaScript object passed to `animate()` or
/// `createTimeline()` holds. Functions are the two kinds the port supports,
/// a `stagger()` and an easing function.
#[derive(Debug, Clone)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(Obj),
    Stagger(Rc<Stagger>),
    Ease(EaseSpec),
}

/// A JavaScript object: keys in insertion order.
#[derive(Debug, Clone, Default)]
pub struct Obj(pub Vec<(String, Value)>);

impl Obj {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn set(&mut self, key: &str, value: Value) {
        match self.0.iter_mut().find(|(k, _)| k == key) {
            Some((_, v)) => *v = value,
            None => self.0.push((key.to_string(), value)),
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(|(k, _)| k.as_str())
    }
}

/// What a property value computes to once functions have run: a number or
/// a string (`'100px'`, `'+=20'`), or the internal `_value` of a tween.
#[derive(Debug, Clone, PartialEq)]
pub enum Raw {
    Undefined,
    Number(f64),
    String(String),
}

impl Raw {
    pub fn is_undefined(&self) -> bool {
        matches!(self, Raw::Undefined)
    }
}

/// `isKey(p)`: whether a parameter name is an animated property rather
/// than one of `globals.defaults`.
pub fn is_key(name: &str) -> bool {
    !matches!(
        name,
        "id" | "keyframes"
            | "playbackEase"
            | "playbackRate"
            | "frameRate"
            | "loop"
            | "reversed"
            | "alternate"
            | "autoplay"
            | "persist"
            | "duration"
            | "delay"
            | "loopDelay"
            | "ease"
            | "composition"
            | "modifier"
            | "onBegin"
            | "onBeforeUpdate"
            | "onUpdate"
            | "onLoop"
            | "onPause"
            | "onComplete"
            | "onRender"
    )
}

/// anime.js `round(v, decimalLength)`.
pub fn round(v: f64, decimals: i32) -> f64 {
    if decimals < 0 {
        return v;
    }
    if decimals == 0 {
        return math_round(v);
    }
    let p = 10f64.powi(decimals);
    math_round(v * p) / p
}

/// anime.js `clamp(v, min, max)`.
pub fn clamp(v: f64, min: f64, max: f64) -> f64 {
    if v < min {
        min
    } else if v > max {
        max
    } else {
        v
    }
}

/// anime.js `lerp(start, end, factor)`.
pub fn lerp(start: f64, end: f64, factor: f64) -> f64 {
    if factor == 1.0 {
        end
    } else if factor == 0.0 {
        start
    } else {
        start + (end - start) * factor
    }
}

pub const MIN_VALUE: f64 = 1e-11;
pub const MAX_VALUE: f64 = 1e12;

/// anime.js `clampInfinity(v)`.
pub fn clamp_infinity(v: f64) -> f64 {
    if v == f64::INFINITY {
        MAX_VALUE
    } else if v == f64::NEG_INFINITY {
        -MAX_VALUE
    } else {
        v
    }
}

/// JavaScript `~~x` (ToInt32).
pub fn to_int32(x: f64) -> f64 {
    if !x.is_finite() {
        return 0.0;
    }
    let t = x.trunc() % 4294967296.0;
    let t = if t < 0.0 { t + 4294967296.0 } else { t };
    if t >= 2147483648.0 {
        t - 4294967296.0
    } else {
        t
    }
}

/// JavaScript `+string` (ToNumber applied to a string).
pub fn string_to_number(s: &str) -> f64 {
    let s = s.trim_matches(|c: char| {
        matches!(
            c,
            '\u{9}' | '\u{a}' | '\u{b}' | '\u{c}' | '\u{d}' | ' ' | '\u{a0}' | '\u{feff}'
        ) || c.is_whitespace()
    });
    if s.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(digits) = s.strip_prefix(prefix) {
            if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                return f64::NAN;
            }
            return digits.chars().fold(0.0, |n, c| {
                n * radix as f64 + c.to_digit(radix).unwrap() as f64
            });
        }
    }
    let unsigned = s.strip_prefix(['+', '-']).unwrap_or(s);
    if unsigned == "Infinity" {
        return if s.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    // StrUnsignedDecimalLiteral: digits, optional fraction, optional
    // exponent, with at least one digit in the mantissa.
    let bytes = unsigned.as_bytes();
    let mut i = 0;
    let mut mantissa_digits = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
        mantissa_digits += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
            mantissa_digits += 1;
        }
    }
    if mantissa_digits == 0 {
        return f64::NAN;
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        i += 1;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return f64::NAN;
        }
    }
    if i != bytes.len() {
        return f64::NAN;
    }
    s.parse::<f64>().unwrap_or(f64::NAN)
}

/// `unitsExecRgx`, `/^([-+]?\d*\.?\d+(?:e[-+]?\d+)?)([a-z]+|%)$/i`: the
/// number and the unit of `'100px'`.
pub fn units_exec(s: &str) -> Option<(&str, &str)> {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    // `\d*\.?\d+`: digits, then optionally a dot and at least one digit;
    // or a dot and at least one digit.
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let int_digits = i - int_start;
    if i < b.len() && b[i] == b'.' && i + 1 < b.len() && b[i + 1].is_ascii_digit() {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    } else if int_digits == 0 {
        return None;
    }
    let number_end = i;
    let unit_matches = |from: usize| -> bool {
        let rest = &b[from..];
        rest == b"%" || (!rest.is_empty() && rest.iter().all(u8::is_ascii_alphabetic))
    };
    // `(?:e[-+]?\d+)?` is greedy: try it first, then backtrack without it.
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let digits_start = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > digits_start && unit_matches(j) {
            return Some((&s[..j], &s[j..]));
        }
    }
    if unit_matches(number_end) {
        Some((&s[..number_end], &s[number_end..]))
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    Number,
    Unit,
}

/// A decomposed value (`createDecomposedValueTargetObject`).
#[derive(Debug, Clone, PartialEq)]
pub struct Decomposed {
    pub t: ValueType,
    pub n: f64,
    pub u: Option<String>,
    pub o: Option<char>,
}

/// A value outside the port: colours, complex strings, unit conversions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported(pub String);

/// `decomposeRawValue(rawValue, targetObject)`.
pub fn decompose_raw_value(raw: &Raw) -> Result<Decomposed, Unsupported> {
    let mut target = Decomposed {
        t: ValueType::Number,
        n: 0.0,
        u: None,
        o: None,
    };
    let text = match raw {
        Raw::Undefined => return Ok(target),
        Raw::Number(n) => {
            // `!rawValue` is true for 0 and NaN.
            if *n != 0.0 && !n.is_nan() {
                target.n = *n;
            }
            return Ok(target);
        }
        Raw::String(s) => {
            if s.is_empty() {
                return Ok(target);
            }
            s.as_str()
        }
    };
    let num = string_to_number(text);
    if !num.is_nan() {
        target.n = num;
        return Ok(target);
    }
    let mut str = text;
    let mut chars = str.chars();
    if let (Some(first), Some('=')) = (chars.next(), chars.next()) {
        target.o = Some(first);
        str = &str[first.len_utf8() + 1..];
    }
    let unit = if str.contains(' ') {
        None
    } else {
        units_exec(str)
    };
    if let Some((number, unit)) = unit {
        target.t = ValueType::Unit;
        target.n = string_to_number(number);
        target.u = Some(unit.to_string());
        Ok(target)
    } else if target.o.is_some() {
        target.n = string_to_number(str);
        Ok(target)
    } else {
        Err(Unsupported(format!(
            "colour and complex values are not ported: {text:?}"
        )))
    }
}

/// `getRelativeValue(x, y, operator)`.
pub fn relative_value(x: f64, y: f64, operator: char) -> f64 {
    match operator {
        '-' => x - y,
        '+' => x + y,
        _ => x * y,
    }
}

/// `validTransforms`, in the order `buildTransformString` writes them.
pub const VALID_TRANSFORMS: [&str; 15] = [
    "perspective",
    "translateX",
    "translateY",
    "translateZ",
    "rotate",
    "rotateX",
    "rotateY",
    "rotateZ",
    "scale",
    "scaleX",
    "scaleY",
    "scaleZ",
    "skew",
    "skewX",
    "skewY",
];

/// `shortTransforms`: `x`, `y` and `z`.
pub fn short_transform(name: &str) -> Option<&'static str> {
    match name {
        "x" => Some("translateX"),
        "y" => Some("translateY"),
        "z" => Some("translateZ"),
        _ => None,
    }
}

/// The per-target transform cache (`target[transformsSymbol]`).
pub type Transforms = BTreeMap<String, String>;

/// `parseInlineTransforms(target, propName)`: read the element's inline
/// `transform` into the cache and return the property's value, or its
/// default when the inline transform does not set it.
pub fn parse_inline_transforms(inline: &str, cache: &mut Transforms, prop: &str) -> String {
    if !inline.is_empty() {
        let b = inline.as_bytes();
        let len = b.len();
        let mut pos = 0;
        let mut full_translate: Option<&str> = None;
        let set = |cache: &mut Transforms, key: &str, value: &str| {
            cache.insert(key.to_string(), value.to_string());
        };
        while pos < len {
            while pos < len && b[pos] == b' ' {
                pos += 1;
            }
            if pos >= len {
                break;
            }
            let name_start = pos;
            while pos < len && b[pos] != b'(' {
                pos += 1;
            }
            if pos >= len {
                break;
            }
            let name = &inline[name_start..pos];
            let mut depth = 1;
            let value_start = pos + 1;
            let (mut c1, mut c2) = (None, None);
            pos += 1;
            while pos < len && depth > 0 {
                match b[pos] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    b',' if depth == 1 => {
                        if c1.is_none() {
                            c1 = Some(pos);
                        } else if c2.is_none() {
                            c2 = Some(pos);
                        }
                    }
                    _ => {}
                }
                pos += 1;
            }
            let value_end = pos - 1;
            let trim = |s: &str| s.trim().to_string();
            if name == "translate" || name == "translate3d" {
                match c1 {
                    None => set(cache, "translateX", &trim(&inline[value_start..value_end])),
                    Some(c1) => {
                        set(cache, "translateX", &trim(&inline[value_start..c1]));
                        match c2 {
                            None => set(cache, "translateY", &trim(&inline[c1 + 1..value_end])),
                            Some(c2) => {
                                set(cache, "translateY", &trim(&inline[c1 + 1..c2]));
                                set(cache, "translateZ", &trim(&inline[c2 + 1..value_end]));
                            }
                        }
                    }
                }
                full_translate = Some(&inline[value_start..value_end]);
            } else if name == "scale" || name == "scale3d" {
                match c1 {
                    None => set(cache, "scale", &trim(&inline[value_start..value_end])),
                    Some(c1) => {
                        set(cache, "scaleX", &trim(&inline[value_start..c1]));
                        match c2 {
                            None => set(cache, "scaleY", &trim(&inline[c1 + 1..value_end])),
                            Some(c2) => {
                                set(cache, "scaleY", &trim(&inline[c1 + 1..c2]));
                                set(cache, "scaleZ", &trim(&inline[c2 + 1..value_end]));
                            }
                        }
                    }
                }
            } else {
                set(cache, name, &inline[value_start..value_end]);
            }
        }
        if prop == "translate3d" {
            if let Some(full) = full_translate {
                return full.to_string();
            }
        }
        if let Some(cached) = cache.get(prop) {
            return cached.clone();
        }
    }
    if prop == "translate3d" {
        "0px, 0px, 0px".into()
    } else if prop == "rotate3d" {
        "0, 0, 0, 0deg".into()
    } else if prop.starts_with("scale") {
        "1".into()
    } else if prop.starts_with("rotate") || prop.starts_with("skew") {
        "0deg".into()
    } else {
        "0px".into()
    }
}

/// `buildTransformString(props)`.
pub fn build_transform_string(props: &Transforms) -> String {
    let mut s = String::new();
    let mut i = 0;
    while i < VALID_TRANSFORMS.len() {
        let key = VALID_TRANSFORMS[i];
        if let Some(val) = props.get(key) {
            let mut grouped = false;
            if key == "translateX" {
                if let Some(next) = props.get("translateY") {
                    if let Some(next2) = props.get("translateZ") {
                        s += &format!("translate3d({val},{next},{next2}) ");
                        i += 2;
                    } else {
                        s += &format!("translate({val},{next}) ");
                        i += 1;
                    }
                    grouped = true;
                }
            }
            if !grouped && key == "scaleX" && !props.contains_key("scale") {
                if let Some(next) = props.get("scaleY") {
                    if let Some(next2) = props.get("scaleZ") {
                        s += &format!("scale3d({val},{next},{next2}) ");
                        i += 2;
                    } else {
                        s += &format!("scale({val},{next}) ");
                        i += 1;
                    }
                    grouped = true;
                }
            }
            if grouped {
                // `continue` skips the rotate3d check below.
                i += 1;
                continue;
            }
            s += &format!("{key}({val}) ");
        }
        if key == "rotateZ" {
            if let Some(r) = props.get("rotate3d") {
                s += &format!("rotate3d({r}) ");
            }
        }
        i += 1;
    }
    if let Some(m) = props.get("matrix") {
        s += &format!("matrix({m}) ");
    }
    if let Some(m) = props.get("matrix3d") {
        s += &format!("matrix3d({m}) ");
    }
    s
}

/// JavaScript `${value}` of a tween value.
pub fn raw_to_string(raw: &Raw) -> String {
    match raw {
        Raw::Undefined => "undefined".into(),
        Raw::Number(n) => number_to_string(*n),
        Raw::String(s) => s.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units() {
        assert_eq!(units_exec("100px"), Some(("100", "px")));
        assert_eq!(units_exec("-1.5deg"), Some(("-1.5", "deg")));
        assert_eq!(units_exec(".5em"), Some((".5", "em")));
        assert_eq!(units_exec("1e3px"), Some(("1e3", "px")));
        assert_eq!(units_exec("10em"), Some(("10", "em")));
        assert_eq!(units_exec("5e"), Some(("5", "e")));
        assert_eq!(units_exec("50%"), Some(("50", "%")));
        assert_eq!(units_exec("5.px"), None);
        assert_eq!(units_exec("px"), None);
        assert_eq!(units_exec("100"), None);
    }

    #[test]
    fn to_number() {
        assert_eq!(string_to_number(" 12 "), 12.0);
        assert_eq!(string_to_number(""), 0.0);
        assert_eq!(string_to_number(".5"), 0.5);
        assert_eq!(string_to_number("5."), 5.0);
        assert_eq!(string_to_number("1e3"), 1000.0);
        assert_eq!(string_to_number("0x10"), 16.0);
        assert!(string_to_number("100px").is_nan());
        assert!(string_to_number("inf").is_nan());
        assert_eq!(string_to_number("-Infinity"), f64::NEG_INFINITY);
    }

    #[test]
    fn transforms_round_trip() {
        let mut cache = Transforms::new();
        cache.insert("translateX".into(), "10px".into());
        cache.insert("rotate".into(), "45deg".into());
        cache.insert("scale".into(), "1.5".into());
        assert_eq!(
            build_transform_string(&cache),
            "translateX(10px) rotate(45deg) scale(1.5) "
        );
        cache.insert("translateY".into(), "2px".into());
        assert_eq!(
            build_transform_string(&cache),
            "translate(10px,2px) rotate(45deg) scale(1.5) "
        );
        let mut read = Transforms::new();
        let inline = build_transform_string(&cache);
        assert_eq!(
            parse_inline_transforms(&inline, &mut read, "translateY"),
            "2px"
        );
        assert_eq!(parse_inline_transforms(&inline, &mut read, "skewX"), "0deg");
    }
}
