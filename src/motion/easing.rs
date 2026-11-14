//! Motion's easings (motion-utils 14.7.0 `easing/`) and how motion-dom
//! 14.14.0 maps them onto a native (WAAPI) easing
//! (`animation/waapi/easing/map-easing.mjs`).

use super::js::{math_round, number_to_string};
use std::fmt;
use std::rc::Rc;

/// A Motion easing definition: a name, a cubic Bézier array or a function.
#[derive(Clone)]
pub enum Ease {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    CircIn,
    CircOut,
    CircInOut,
    BackIn,
    BackOut,
    BackInOut,
    Anticipate,
    /// `[x1, y1, x2, y2]`.
    Bezier([f64; 4]),
    /// A JavaScript easing function, `(progress) => progress`.
    Function(Rc<dyn Fn(f64) -> f64>),
}

impl fmt::Debug for Ease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bezier(points) => write!(f, "Bezier({points:?})"),
            Self::Function(_) => f.write_str("Function"),
            named => f.write_str(named.name().unwrap_or("?")),
        }
    }
}

impl Ease {
    /// The name Motion uses for a named easing.
    pub fn name(&self) -> Option<&'static str> {
        Some(match self {
            Self::Linear => "linear",
            Self::EaseIn => "easeIn",
            Self::EaseOut => "easeOut",
            Self::EaseInOut => "easeInOut",
            Self::CircIn => "circIn",
            Self::CircOut => "circOut",
            Self::CircInOut => "circInOut",
            Self::BackIn => "backIn",
            Self::BackOut => "backOut",
            Self::BackInOut => "backInOut",
            Self::Anticipate => "anticipate",
            Self::Bezier(_) | Self::Function(_) => return None,
        })
    }

    /// `replaceStringEasing`: the three named easings WAAPI has no
    /// equivalent for become their JavaScript functions, which are later
    /// exported as `linear()`.
    pub fn replace_unsupported(self) -> Self {
        match self {
            Self::Anticipate => Self::Function(Rc::new(anticipate)),
            Self::BackInOut => Self::Function(Rc::new(back_in_out)),
            Self::CircInOut => Self::Function(Rc::new(circ_in_out)),
            other => other,
        }
    }
}

/// A native easing, as passed to `element.animate()`.
#[derive(Debug, Clone, PartialEq)]
pub enum NativeEasing {
    /// `linear`, `ease`, `ease-in`, `ease-out` or `ease-in-out`.
    Keyword(&'static str),
    CubicBezier([f64; 4]),
    /// `linear(o0, o1, ...)` with evenly spaced, implicit inputs.
    Linear(Vec<f64>),
}

impl NativeEasing {
    /// The string Motion passes to WAAPI.
    pub fn to_css(&self) -> String {
        match self {
            Self::Keyword(keyword) => (*keyword).to_owned(),
            Self::CubicBezier([a, b, c, d]) => format!(
                "cubic-bezier({}, {}, {}, {})",
                number_to_string(*a),
                number_to_string(*b),
                number_to_string(*c),
                number_to_string(*d)
            ),
            Self::Linear(outputs) => {
                let points: Vec<String> = outputs.iter().map(|v| number_to_string(*v)).collect();
                format!("linear({})", points.join(", "))
            }
        }
    }
}

/// `supportedWaapiEasing`: Motion's names that WAAPI can express directly.
pub fn supported_waapi_easing(ease: &Ease) -> Option<NativeEasing> {
    Some(match ease {
        Ease::Linear => NativeEasing::Keyword("linear"),
        Ease::EaseIn => NativeEasing::Keyword("ease-in"),
        Ease::EaseOut => NativeEasing::Keyword("ease-out"),
        Ease::EaseInOut => NativeEasing::Keyword("ease-in-out"),
        Ease::CircIn => NativeEasing::CubicBezier([0.0, 0.65, 0.55, 1.0]),
        Ease::CircOut => NativeEasing::CubicBezier([0.55, 0.0, 1.0, 0.45]),
        Ease::BackIn => NativeEasing::CubicBezier([0.31, 0.01, 0.66, -0.59]),
        Ease::BackOut => NativeEasing::CubicBezier([0.33, 1.53, 0.69, 0.99]),
        _ => return None,
    })
}

/// `generateLinearEasing(easing, duration, resolution)`.
pub fn generate_linear_easing(
    easing: &dyn Fn(f64) -> f64,
    duration_ms: f64,
    resolution_ms: f64,
) -> NativeEasing {
    let points = math_round(duration_ms / resolution_ms).max(2.0) as usize;
    let outputs = (0..points)
        .map(|i| math_round(easing(i as f64 / (points - 1) as f64) * 10000.0) / 10000.0)
        .collect();
    NativeEasing::Linear(outputs)
}

/// `mapEasingToNativeEasing` for a single (non-array) easing. Returns `None`
/// where Motion's lookup is `undefined`.
pub fn map_easing_to_native(ease: &Ease, duration_ms: f64) -> Option<NativeEasing> {
    match ease {
        Ease::Function(function) => {
            Some(generate_linear_easing(function.as_ref(), duration_ms, 10.0))
        }
        Ease::Bezier(points) => Some(NativeEasing::CubicBezier(*points)),
        named => supported_waapi_easing(named),
    }
}

// motion-utils easing functions.

/// motion-utils `cubicBezier(x1, y1, x2, y2)`: Newton-Raphson with a forward
/// difference slope, safeguarded by bisection.
pub fn cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64) -> impl Fn(f64) -> f64 {
    let linear = x1 == y1 && x2 == y2;
    move |x: f64| {
        if linear {
            return x;
        }
        let mut lower = 0.0;
        let mut upper = 1.0;
        let mut t = x;
        let mut i = 0;
        loop {
            // `!(t >= lower && t <= upper)`, which NaN also satisfies.
            if t.is_nan() || t < lower || t > upper {
                t = (lower + upper) / 2.0;
            }
            let current_x = calc_bezier(t, x1, x2) - x;
            if current_x * current_x < 1e-28 || i == 40 {
                break;
            }
            if current_x > 0.0 {
                upper = t;
            } else {
                lower = t;
            }
            t -= (current_x * 1e-7) / (calc_bezier(t + 1e-7, x1, x2) - x - current_x);
            i += 1;
        }
        calc_bezier(t, y1, y2)
    }
}

fn calc_bezier(t: f64, a1: f64, a2: f64) -> f64 {
    t * (3.0 * (1.0 - t) * ((1.0 - t) * a1 + t * a2) + t * t)
}

fn mirror(easing: impl Fn(f64) -> f64) -> impl Fn(f64) -> f64 {
    move |p| {
        if p <= 0.5 {
            easing(2.0 * p) / 2.0
        } else {
            (2.0 - easing(2.0 * (1.0 - p))) / 2.0
        }
    }
}

fn reverse(easing: impl Fn(f64) -> f64) -> impl Fn(f64) -> f64 {
    move |p| 1.0 - easing(1.0 - p)
}

pub fn back_out(p: f64) -> f64 {
    cubic_bezier(0.33, 1.53, 0.69, 0.99)(p)
}

pub fn back_in(p: f64) -> f64 {
    reverse(back_out)(p)
}

pub fn back_in_out(p: f64) -> f64 {
    mirror(back_in)(p)
}

pub fn circ_in(p: f64) -> f64 {
    1.0 - p.acos().sin()
}

pub fn circ_in_out(p: f64) -> f64 {
    mirror(circ_in)(p)
}

pub fn anticipate(p: f64) -> f64 {
    if p >= 1.0 {
        return 1.0;
    }
    let p = p * 2.0;
    if p < 1.0 {
        0.5 * back_in(p)
    } else {
        0.5 * (2.0 - 2.0_f64.powf(-10.0 * (p - 1.0)))
    }
}
