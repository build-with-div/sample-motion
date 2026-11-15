//! anime.js's easings: the string eases of `easings/eases/parser.js` and the
//! easing functions `cubicBezier`, `steps` and `linear`.
//!
//! The formulas use `Math.pow`, `Math.sin`, `Math.cos` and `Math.asin`.
//! The port calls Rust's `f64` functions for them, which are not V8's;
//! anime.js rounds every in-between value to four decimals before writing
//! it, and tests/anime_values.rs compares the written strings.

use super::value::{clamp, string_to_number, Raw, Unsupported, MIN_VALUE};
use crate::motion::js::parse_float;
use std::f64::consts::PI;
use std::rc::Rc;

/// An easing function as a parameter: a string (`'out(2)'`,
/// `'inOutSine'`) or one of the function eases.
#[derive(Debug, Clone, PartialEq)]
pub enum EaseSpec {
    Name(String),
    /// `cubicBezier(x1, y1, x2, y2)`.
    CubicBezier(Vec<f64>),
    /// `steps(steps, fromStart)`.
    Steps(Option<f64>, bool),
    /// `linear(...points)`.
    Linear(Vec<Raw>),
}

/// A parsed easing function.
pub type EaseFn = Rc<dyn Fn(f64) -> f64>;

fn none() -> EaseFn {
    Rc::new(|t| t)
}

/// `parseEase(ease)`.
pub fn parse_ease(spec: &EaseSpec) -> Result<EaseFn, Unsupported> {
    match spec {
        EaseSpec::Name(name) => parse_ease_string(name),
        EaseSpec::CubicBezier(args) => Ok(cubic_bezier(args)),
        EaseSpec::Steps(steps, from_start) => Ok(steps_ease(steps.unwrap_or(10.0), *from_start)),
        EaseSpec::Linear(points) => Ok(linear(points)),
    }
}

#[derive(Clone, Copy)]
enum EaseType {
    In,
    Out,
    InOut,
    OutIn,
}

fn ease_type(name: &str) -> Option<(EaseType, &str)> {
    // `for (let type in easeTypes)`: the first type whose name is a prefix
    // and whose rest is an ease-in function name builds the entry.
    for (prefix, ty) in [
        ("inOut", EaseType::InOut),
        ("outIn", EaseType::OutIn),
        ("in", EaseType::In),
        ("out", EaseType::Out),
    ] {
        if let Some(rest) = name.strip_prefix(prefix) {
            if matches!(
                rest,
                "" | "Quad"
                    | "Cubic"
                    | "Quart"
                    | "Quint"
                    | "Sine"
                    | "Circ"
                    | "Expo"
                    | "Bounce"
                    | "Back"
                    | "Elastic"
            ) {
                return Some((ty, rest));
            }
        }
    }
    None
}

fn apply_type(ty: EaseType, ease_in: EaseFn) -> EaseFn {
    match ty {
        EaseType::In => Rc::new(move |t| ease_in(t)),
        EaseType::Out => Rc::new(move |t| 1.0 - ease_in(1.0 - t)),
        EaseType::InOut => Rc::new(move |t| {
            if t < 0.5 {
                ease_in(t * 2.0) / 2.0
            } else {
                1.0 - ease_in(t * -2.0 + 2.0) / 2.0
            }
        }),
        EaseType::OutIn => Rc::new(move |t| {
            if t < 0.5 {
                (1.0 - ease_in(1.0 - t * 2.0)) / 2.0
            } else {
                (ease_in(t * 2.0 - 1.0) + 1.0) / 2.0
            }
        }),
    }
}

fn ease_in_power(p: f64) -> EaseFn {
    Rc::new(move |t| t.powf(p))
}

/// `+argument`, or the default when the argument is missing.
fn arg(args: &[Option<String>], i: usize, default: f64) -> f64 {
    match args.get(i) {
        Some(Some(s)) => string_to_number(s),
        _ => default,
    }
}

/// The ease-in function `easeInFunctions[name]`, called with `args` for
/// the parametric ones.
fn ease_in(name: &str, args: &[Option<String>]) -> EaseFn {
    let half_pi = PI / 2.0;
    let double_pi = PI * 2.0;
    match name {
        "" => ease_in_power(arg(args, 0, 1.68)),
        "Quad" => ease_in_power(2.0),
        "Cubic" => ease_in_power(3.0),
        "Quart" => ease_in_power(4.0),
        "Quint" => ease_in_power(5.0),
        "Sine" => Rc::new(move |t: f64| 1.0 - (t * half_pi).cos()),
        "Circ" => Rc::new(|t: f64| 1.0 - (1.0 - t * t).sqrt()),
        "Expo" => Rc::new(|t: f64| {
            if t != 0.0 && !t.is_nan() {
                2f64.powf(10.0 * t - 10.0)
            } else {
                0.0
            }
        }),
        // `while (t < x)` stops for NaN too, so the negated comparison stays.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        "Bounce" => Rc::new(|t: f64| {
            let mut b = 4.0;
            let mut pow2;
            loop {
                b -= 1.0;
                pow2 = 2f64.powf(b);
                if !(t < (pow2 - 1.0) / 11.0) {
                    break;
                }
            }
            1.0 / 4f64.powf(3.0 - b) - 7.5625 * ((pow2 * 3.0 - 2.0) / 22.0 - t).powf(2.0)
        }),
        "Back" => {
            let overshoot = arg(args, 0, 1.7);
            Rc::new(move |t: f64| (overshoot + 1.0) * t * t * t - overshoot * t * t)
        }
        "Elastic" => {
            let a = clamp(arg(args, 0, 1.0), 1.0, 10.0);
            let p = clamp(arg(args, 1, 0.3), MIN_VALUE, 2.0);
            let s = (p / double_pi) * (1.0 / a).asin();
            let e = double_pi / p;
            Rc::new(move |t: f64| {
                if t == 0.0 || t == 1.0 {
                    t
                } else {
                    -a * 2f64.powf(-10.0 * (1.0 - t)) * (((1.0 - t) - s) * e).sin()
                }
            })
        }
        _ => unreachable!("checked by ease_type"),
    }
}

/// `parseEaseString(string)`.
fn parse_ease_string(string: &str) -> Result<EaseFn, Unsupported> {
    if string == "linear" || string == "none" {
        return Ok(none());
    }
    let parametric = |rest: &str| rest.is_empty() || rest == "Back" || rest == "Elastic";
    match string.find('(') {
        None => match ease_type(string) {
            Some((ty, rest)) => Ok(apply_type(ty, ease_in(rest, &[]))),
            None if string.contains("Back") || string.contains("Elastic") => Err(Unsupported(
                format!("anime.js throws for the unknown ease {string:?}"),
            )),
            None => Ok(none()),
        },
        Some(open) => {
            // `string.slice(0, -1).split('(')`: the name, then the text up
            // to the next `(`, split at commas.
            let inner = &string[..string.len() - string.chars().last().map_or(0, char::len_utf8)];
            let name = &inner[..open];
            let rest = &inner[open + 1..];
            let rest = rest.split('(').next().unwrap_or("");
            let args: Vec<Option<String>> = rest.split(',').map(|s| Some(s.to_string())).collect();
            match ease_type(name) {
                Some((ty, ease)) if parametric(ease) => Ok(apply_type(ty, ease_in(ease, &args))),
                Some(_) => Err(Unsupported(format!(
                    "{name:?} takes no parameters; anime.js would use a number as the ease"
                ))),
                None => Ok(none()),
            }
        }
    }
}

fn calc_bezier(t: f64, a1: f64, a2: f64) -> f64 {
    (((1.0 - 3.0 * a2 + 3.0 * a1) * t + (3.0 * a2 - 6.0 * a1)) * t + (3.0 * a1)) * t
}

fn binary_subdivide(x: f64, x1: f64, x2: f64) -> f64 {
    let (mut a, mut b) = (0.0, 1.0);
    let mut i = 0;
    loop {
        let current_t = a + (b - a) / 2.0;
        let current_x = calc_bezier(current_t, x1, x2) - x;
        if current_x > 0.0 {
            b = current_t;
        } else {
            a = current_t;
        }
        i += 1;
        if !(current_x.abs() > 0.0000001 && i < 100) {
            return current_t;
        }
    }
}

/// `cubicBezier(mX1 = 0.5, mY1 = 0, mX2 = 0.5, mY2 = 1)`.
fn cubic_bezier(args: &[f64]) -> EaseFn {
    let get = |i: usize, d: f64| args.get(i).copied().unwrap_or(d);
    let (x1, y1, x2, y2) = (get(0, 0.5), get(1, 0.0), get(2, 0.5), get(3, 1.0));
    if x1 == y1 && x2 == y2 {
        return none();
    }
    Rc::new(move |t| {
        if t == 0.0 || t == 1.0 {
            t
        } else {
            calc_bezier(binary_subdivide(t, x1, x2), y1, y2)
        }
    })
}

/// `steps(steps = 10, fromStart)`.
fn steps_ease(steps: f64, from_start: bool) -> EaseFn {
    Rc::new(move |t| {
        let v = clamp(t, 0.0, 1.0) * steps;
        let r = if from_start { v.ceil() } else { v.floor() };
        r * (1.0 / steps)
    })
}

/// `parseNumber(v)`: `parseFloat` for strings.
fn parse_number(raw: &Raw) -> f64 {
    match raw {
        Raw::Number(n) => *n,
        Raw::String(s) => parse_float(s),
        Raw::Undefined => f64::NAN,
    }
}

/// `linear(...args)`.
fn linear(args: &[Raw]) -> EaseFn {
    if args.is_empty() {
        return none();
    }
    let total = args.len() - 1;
    let mut xs = vec![0.0];
    let mut ys = vec![parse_number(&args[0])];
    for (i, arg) in args.iter().enumerate().take(total).skip(1) {
        let (value, percent) = match arg {
            Raw::String(s) => {
                let mut parts = s.trim().split(' ');
                let value = parts.next().unwrap_or("").to_string();
                (Raw::String(value), parts.next().map(str::to_string))
            }
            other => (other.clone(), None),
        };
        xs.push(match percent {
            Some(p) => parse_float(&p) / 100.0,
            None => i as f64 / total as f64,
        });
        ys.push(parse_number(&value));
    }
    ys.push(parse_number(&args[total]));
    xs.push(1.0);
    Rc::new(move |t| {
        for i in 1..xs.len() {
            if t <= xs[i] {
                let (px, py) = (xs[i - 1], ys[i - 1]);
                return py + (ys[i] - py) * (t - px) / (xs[i] - px);
            }
        }
        ys[ys.len() - 1]
    })
}
