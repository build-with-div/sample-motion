//! Turning a generator into a WAAPI easing (motion-dom 14.14.0
//! `generators/utils/calc-duration.mjs`, `create-generator-easing.mjs` and
//! `spring.applyToOptions`).

use super::js::{milliseconds_to_seconds, seconds_to_milliseconds};
use super::spring::{Spring, SpringOptions};
use std::rc::Rc;

/// `maxGeneratorDuration`.
pub const MAX_GENERATOR_DURATION: f64 = 20000.0;

/// `calcGeneratorDuration(generator)` with its default 50 ms step: the first
/// multiple of 50 ms at which the generator reports done, or infinity.
pub fn calc_generator_duration(next: impl Fn(f64) -> (f64, bool)) -> f64 {
    let time_step = 50.0;
    let mut duration = 0.0;
    let mut state = next(duration);
    while !state.1 && duration < MAX_GENERATOR_DURATION {
        duration += time_step;
        state = next(duration);
    }
    if duration >= MAX_GENERATOR_DURATION {
        f64::INFINITY
    } else {
        duration
    }
}

/// What `spring.applyToOptions` writes back into the transition: a
/// progress easing and a duration in milliseconds.
#[derive(Clone)]
pub struct GeneratorEasing {
    pub ease: Rc<dyn Fn(f64) -> f64>,
    /// `options.duration`, milliseconds.
    pub duration_ms: f64,
}

/// `createGeneratorEasing(options, 100, spring)` followed by the
/// milliseconds conversion in `spring.applyToOptions`.
///
/// `keyframe_delta` is `keyframes[last] - keyframes[0]` (NaN for keyframes
/// with units), used only to scale an initial velocity.
pub fn spring_easing(options: &SpringOptions, keyframe_delta: f64) -> GeneratorEasing {
    let scale = 100.0;
    let mut scaled = options.clone();
    // `v && delta ? (v * scale) / delta : v`
    scaled.velocity = match options.velocity {
        Some(v) if v != 0.0 && !v.is_nan() && keyframe_delta != 0.0 && !keyframe_delta.is_nan() => {
            Some((v * scale) / keyframe_delta)
        }
        other => other,
    };
    let generator = Spring::new(&scaled, 0.0, scale);
    let duration = calc_generator_duration(|t| generator.next(t)).min(MAX_GENERATOR_DURATION);
    let duration_seconds = milliseconds_to_seconds(duration);
    GeneratorEasing {
        ease: Rc::new(move |progress| generator.next(duration * progress).0 / scale),
        duration_ms: seconds_to_milliseconds(duration_seconds),
    }
}
