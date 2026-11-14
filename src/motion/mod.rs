//! A port of Motion's `animate()` core (the `motion` npm package, 14.14.0;
//! motion-dom 14.14.0, motion-utils 14.7.0) for the values Motion runs on
//! the Web Animations API: `opacity` and `transform`.
//!
//! The port keeps Motion's own formulas and operation order: transition
//! resolution, the spring generator, its `linear()` export and the easing
//! mapping. [`native::resolve`] returns exactly the `element.animate()`
//! call Motion makes, which `tools/motion_oracle.py` checks against Motion
//! running in pinned Chromium. [`animate`] then makes that call on Div.
//!
//! Div's single-property `Element::animate` stands in for the Web Animations
//! API (plan A3). Values Motion runs on its own frame loop (inertia,
//! `repeatDelay`, `repeatType: "mirror"`, `damping: 0`, and every value
//! other than opacity, transform, filter and clip-path) need
//! `requestAnimationFrame` (plan R1) and are reported as [`NotNative`].

pub mod easing;
pub mod generator;
pub mod js;
pub mod native;
pub mod spring;

use div::prelude::*;
use div::typed_style::{
    AnimationOptions, Easing, FillMode, IterationCount, Keyframe, Keyframes, PercentageLinearStop,
    PlaybackDirection, TransformList,
};
pub use easing::{Ease, NativeEasing};
pub use native::{
    AcceleratedValue, Direction, EaseDefinition, NativeAnimation, NotNative, Outcome, RepeatType,
    Transition, TransitionType,
};

/// Keyframes for one accelerated value.
#[derive(Debug, Clone)]
pub enum Value {
    Opacity(Vec<f32>),
    Transform(Vec<TransformList>),
}

impl Value {
    fn name(&self) -> AcceleratedValue {
        match self {
            Self::Opacity(_) => AcceleratedValue::Opacity,
            Self::Transform(_) => AcceleratedValue::Transform,
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Opacity(values) => values.len(),
            Self::Transform(values) => values.len(),
        }
    }

    /// `hasKeyframesChanged`.
    fn changed(&self) -> bool {
        fn any_differs<T: PartialEq>(values: &[T]) -> bool {
            values.len() == 1 || values.iter().any(|value| value != &values[0])
        }
        match self {
            Self::Opacity(values) => any_differs(values),
            Self::Transform(values) => any_differs(values),
        }
    }

    /// `keyframes[last] - keyframes[0]`, NaN for values with units.
    fn delta(&self) -> f64 {
        match self {
            Self::Opacity(values) => match (values.first(), values.last()) {
                (Some(first), Some(last)) => f64::from(*last) - f64::from(*first),
                _ => f64::NAN,
            },
            Self::Transform(_) => f64::NAN,
        }
    }
}

#[derive(Debug)]
pub enum Error {
    NotNative(NotNative),
    Div(String),
}

impl From<NotNative> for Error {
    fn from(value: NotNative) -> Self {
        Self::NotNative(value)
    }
}

/// What [`animate`] did for a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Started {
    Animation(div::AnimationId),
    /// The final keyframe was written without an animation.
    Instant,
}

/// What Motion does for `value` under `transition`: the `element.animate()`
/// call it makes, an instant update, or a main-thread animation.
pub fn resolve(value: &Value, transition: &Transition) -> Result<Outcome, NotNative> {
    native::resolve(
        value.name(),
        value.len(),
        value.changed(),
        value.delta(),
        transition,
    )
}

/// Motion's `animate(element, { [value]: keyframes }, transition)` for one
/// accelerated value.
pub fn animate(
    element: &Element,
    value: &Value,
    transition: &Transition,
) -> Result<Started, Error> {
    if value.len() == 0 {
        return Err(Error::Div("no keyframes".into()));
    }
    let outcome = resolve(value, transition)?;
    let native = match outcome {
        Outcome::Native(native) => native,
        Outcome::Instant => {
            // Motion writes the final keyframe on its next frame
            // (frame.update), which here is the next rendered frame.
            let result = match value {
                Value::Opacity(values) => {
                    element.set_property(StyleProperty::Opacity, values[values.len() - 1].into())
                }
                Value::Transform(values) => element.set_property(
                    StyleProperty::Transform,
                    values[values.len() - 1].clone().into(),
                ),
            };
            result.map_err(|error| Error::Div(format!("{error:?}")))?;
            return Ok(Started::Instant);
        }
    };
    let options = AnimationOptions {
        delay_ms: native.delay_ms,
        duration_ms: native.duration_ms,
        iterations: if native.iterations.is_infinite() {
            IterationCount::Infinite
        } else {
            IterationCount::Number(native.iterations)
        },
        direction: match native.direction {
            Direction::Normal => PlaybackDirection::Normal,
            Direction::Alternate => PlaybackDirection::Alternate,
        },
        fill: FillMode::Both,
        easing: to_div_easing(&native.easing),
        ..AnimationOptions::default()
    };
    // div-gap: A3 Web Animations API. One `Element::animate` per property
    // with an `AnimationId` stands in for the `Animation` handle. Motion's
    // onfinish commits the final style and cancels the animation; without
    // `finished`/`onfinish` the filling animation stays instead, which
    // renders the same final value.
    let id = match value {
        Value::Opacity(values) => {
            element.animate(StyleProperty::Opacity, keyframes(values, &native)?, options)
        }
        Value::Transform(values) => element.animate(
            StyleProperty::Transform,
            keyframes(values, &native)?,
            options,
        ),
    }
    .map_err(|error| Error::Div(format!("{error:?}")))?;
    Ok(Started::Animation(id))
}

/// Property-indexed keyframes as WAAPI builds them: offsets from `times`
/// (or evenly spaced, as Blink computes missing offsets), and per-keyframe
/// easings repeated cyclically when the list is shorter.
fn keyframes<T: Clone>(values: &[T], native: &NativeAnimation) -> Result<Keyframes<T>, Error> {
    let count = values.len();
    let frames: Vec<Keyframe<T>> = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let offset = native
                .offsets
                .as_ref()
                .and_then(|offsets| offsets.get(index).copied())
                .unwrap_or(if count == 1 {
                    1.0
                } else {
                    index as f64 / (count - 1) as f64
                });
            let mut frame = Keyframe::new(offset, value.clone());
            if let Some(easings) = &native.keyframe_easings {
                if !easings.is_empty() {
                    frame = frame.easing(to_div_easing(&easings[index % easings.len()]));
                }
            }
            frame
        })
        .collect();
    if count == 1 {
        // A single keyframe animates from the underlying value; not
        // expressible with Div's keyframes yet.
        return Err(Error::Div("single keyframe (implicit from-value)".into()));
    }
    Keyframes::new(frames).map_err(|error| Error::Div(format!("{error:?}")))
}

/// A native easing as Chromium parses it.
pub fn to_div_easing(easing: &NativeEasing) -> Easing {
    match easing {
        NativeEasing::Keyword("ease") => Easing::CubicBezier {
            x1: 0.25,
            y1: 0.1,
            x2: 0.25,
            y2: 1.0,
        },
        NativeEasing::Keyword("ease-in") => Easing::CubicBezier {
            x1: 0.42,
            y1: 0.0,
            x2: 1.0,
            y2: 1.0,
        },
        NativeEasing::Keyword("ease-out") => Easing::CubicBezier {
            x1: 0.0,
            y1: 0.0,
            x2: 0.58,
            y2: 1.0,
        },
        NativeEasing::Keyword("ease-in-out") => Easing::CubicBezier {
            x1: 0.42,
            y1: 0.0,
            x2: 0.58,
            y2: 1.0,
        },
        NativeEasing::Keyword(_) => Easing::Linear,
        NativeEasing::CubicBezier([x1, y1, x2, y2]) => Easing::CubicBezier {
            x1: *x1,
            y1: *y1,
            x2: *x2,
            y2: *y2,
        },
        NativeEasing::Linear(outputs) => Easing::LinearStopsPercent(linear_inputs(outputs)),
    }
}

/// Inputs for `linear(o0, o1, ...)` written without percentages, filled as
/// Blink's `ConsumeLinear` does: the first is 0%, the last 100%, and each
/// missing one is `previous + (next - previous) / (distance)`.
fn linear_inputs(outputs: &[f64]) -> Vec<PercentageLinearStop> {
    let count = outputs.len();
    let mut inputs = vec![f64::NAN; count];
    if count > 0 {
        inputs[0] = 0.0;
    }
    if count > 1 {
        inputs[count - 1] = 100.0;
    }
    let upper = count.saturating_sub(1);
    for index in 1..upper {
        inputs[index] =
            inputs[index - 1] + (inputs[upper] - inputs[index - 1]) / (upper - (index - 1)) as f64;
    }
    outputs
        .iter()
        .zip(inputs)
        .map(|(output, input_percent)| PercentageLinearStop {
            input_percent,
            output: *output,
        })
        .collect()
}
