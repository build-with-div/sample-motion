//! What Motion asks the platform for: the arguments motion-dom 14.14.0
//! passes to `element.animate()` for a value it runs on WAAPI.
//!
//! The steps follow `animateMotionValue` (interfaces/motion-value.mjs),
//! `AsyncMotionValueAnimation`, `NativeAnimationExtended`,
//! `NativeAnimation`, `applyGeneratorOptions` and `startWaapiAnimation`.

use super::easing::{map_easing_to_native, Ease, NativeEasing};
use super::generator::spring_easing;
use super::js::seconds_to_milliseconds;
use super::spring::SpringOptions;
use std::rc::Rc;

/// A Motion transition, in Motion's units (seconds). `None` is `undefined`.
#[derive(Debug, Clone, Default)]
pub struct Transition {
    pub kind: Option<TransitionType>,
    pub duration: Option<f64>,
    pub delay: Option<f64>,
    pub ease: Option<EaseDefinition>,
    pub times: Option<Vec<f64>>,
    pub repeat: Option<f64>,
    pub repeat_type: Option<RepeatType>,
    pub repeat_delay: Option<f64>,
    pub stiffness: Option<f64>,
    pub damping: Option<f64>,
    pub mass: Option<f64>,
    pub bounce: Option<f64>,
    pub visual_duration: Option<f64>,
    pub rest_speed: Option<f64>,
    pub rest_delta: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionType {
    Tween,
    Spring,
    Inertia,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatType {
    Loop,
    Reverse,
    Mirror,
}

/// `ease`: one easing for the whole animation, or one per segment.
#[derive(Debug, Clone)]
pub enum EaseDefinition {
    One(Ease),
    PerSegment(Vec<Ease>),
}

/// Values Motion runs on WAAPI (`acceleratedValues`) that this port covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceleratedValue {
    Opacity,
    Transform,
}

impl AcceleratedValue {
    pub fn name(self) -> &'static str {
        match self {
            Self::Opacity => "opacity",
            Self::Transform => "transform",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Normal,
    Alternate,
}

/// What Motion does for a value.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// It calls `element.animate(keyframes, options)`.
    Native(NativeAnimation),
    /// It writes the final keyframe on its next frame without animating
    /// (an instant transition with no delay, or unchanged keyframes).
    Instant,
}

/// The `element.animate(keyframes, options)` call Motion makes.
#[derive(Debug, Clone, PartialEq)]
pub struct NativeAnimation {
    /// `keyframeOptions.offset` (from `times`).
    pub offsets: Option<Vec<f64>>,
    /// `keyframeOptions.easing`, when `ease` is per segment.
    pub keyframe_easings: Option<Vec<NativeEasing>>,
    pub easing: NativeEasing,
    pub delay_ms: f64,
    pub duration_ms: f64,
    /// `repeat + 1`; infinite for `repeat: Infinity`.
    pub iterations: f64,
    pub direction: Direction,
}

/// Why a value does not run on WAAPI in Motion, so the port cannot express
/// it with a native animation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotNative {
    /// Motion runs it as a JSAnimation on its frame loop:
    /// `requestAnimationFrame`, which Div lacks (plan R1).
    MainThread(&'static str),
}

fn is_transition_defined(transition: &Transition) -> bool {
    // Every key except the orchestration keys (delay, repeat, repeatType,
    // repeatDelay, ...) counts.
    transition.kind.is_some()
        || transition.duration.is_some()
        || transition.ease.is_some()
        || transition.times.is_some()
        || transition.stiffness.is_some()
        || transition.damping.is_some()
        || transition.mass.is_some()
        || transition.bounce.is_some()
        || transition.visual_duration.is_some()
        || transition.rest_speed.is_some()
        || transition.rest_delta.is_some()
}

/// The working options object, as it is completed step by step.
struct Options {
    kind: TransitionType,
    duration: Option<f64>,
    delay: f64,
    ease: Option<EaseDefinition>,
    times: Option<Vec<f64>>,
    repeat: f64,
    repeat_type: RepeatType,
    repeat_delay: f64,
    spring: SpringOptions,
}

/// Resolve the native animation Motion starts for `name` animating through
/// `keyframes`. `changed` is whether any keyframe differs from the first
/// (`hasKeyframesChanged`); `delta` is last minus first for numeric
/// keyframes and NaN otherwise.
pub fn resolve(
    name: AcceleratedValue,
    keyframe_count: usize,
    changed: bool,
    delta: f64,
    transition: &Transition,
) -> Result<Outcome, NotNative> {
    // animateMotionValue: `{ ease: "easeOut", ...valueTransition, delay }`.
    let delay_seconds = transition
        .delay
        .filter(|d| *d != 0.0 && !d.is_nan())
        .unwrap_or(0.0);
    let mut options = Options {
        kind: transition.kind.unwrap_or(TransitionType::Tween),
        duration: transition.duration,
        delay: seconds_to_milliseconds(delay_seconds),
        ease: Some(
            transition
                .ease
                .clone()
                .unwrap_or(EaseDefinition::One(Ease::EaseOut)),
        ),
        times: transition.times.clone(),
        repeat: transition.repeat.unwrap_or(0.0),
        repeat_type: transition.repeat_type.unwrap_or(RepeatType::Loop),
        repeat_delay: transition.repeat_delay.unwrap_or(0.0),
        spring: SpringOptions {
            stiffness: transition.stiffness,
            damping: transition.damping,
            mass: transition.mass,
            duration: None,
            bounce: transition.bounce,
            visual_duration: transition.visual_duration,
            // A new element's motion value has no velocity.
            velocity: Some(0.0),
            rest_speed: transition.rest_speed,
            rest_delta: transition.rest_delta,
        },
    };
    if !is_transition_defined(transition) {
        // getDefaultTransition.
        options.kind = TransitionType::Tween;
        if keyframe_count > 2 {
            options.duration = Some(0.8);
        } else {
            match name {
                // Neither is in transformProps (x, scale, rotate, ...), so
                // both take the default `ease` transition.
                AcceleratedValue::Opacity | AcceleratedValue::Transform => {
                    options.ease = Some(EaseDefinition::One(Ease::Bezier([0.25, 0.1, 0.35, 1.0])));
                    options.duration = Some(0.3);
                }
            }
        }
    }
    // `options.duration && (options.duration = secondsToMilliseconds(...))`
    if let Some(duration) = options.duration.filter(|d| *d != 0.0 && !d.is_nan()) {
        options.duration = Some(seconds_to_milliseconds(duration));
    }
    if options.repeat_delay != 0.0 && !options.repeat_delay.is_nan() {
        options.repeat_delay = seconds_to_milliseconds(options.repeat_delay);
    }
    if options.duration == Some(0.0) && options.repeat_delay == 0.0 {
        // With no delay Motion would skip the animation and set the final
        // keyframe, but only when the motion value already holds a value.
        // A value created by this call does not, so it still runs on WAAPI
        // with duration 0 (recorded: tests/motion case `instant`).
        make_instant(&mut options);
    }
    // AsyncMotionValueAnimation.onKeyframesResolved: a value that cannot
    // animate becomes an instant JSAnimation, never WAAPI.
    if !changed {
        return Ok(Outcome::Instant);
    }
    // supportsBrowserAnimation.
    if options.repeat_delay != 0.0 {
        return Err(NotNative::MainThread("repeatDelay"));
    }
    if options.repeat_type == RepeatType::Mirror {
        return Err(NotNative::MainThread("repeatType: mirror"));
    }
    if transition.damping == Some(0.0) {
        return Err(NotNative::MainThread("damping: 0"));
    }
    if options.kind == TransitionType::Inertia {
        return Err(NotNative::MainThread("type: inertia"));
    }
    // NativeAnimationExtended: replaceStringEasing.
    // Only a single named easing is replaced; an array's names are not.
    if let Some(EaseDefinition::One(ease)) = &options.ease {
        options.ease = Some(EaseDefinition::One(ease.clone().replace_unsupported()));
    }
    // applyGeneratorOptions.
    let (ease, duration) = if options.kind == TransitionType::Spring {
        let mut spring = options.spring.clone();
        spring.duration = options.duration;
        let generated = spring_easing(&spring, delta);
        (
            EaseDefinition::One(Ease::Function(Rc::clone(&generated.ease))),
            generated.duration_ms,
        )
    } else {
        (
            options.ease.unwrap_or(EaseDefinition::One(Ease::EaseOut)),
            options.duration.unwrap_or(300.0),
        )
    };
    // startWaapiAnimation.
    let (easing, keyframe_easings) = match &ease {
        EaseDefinition::One(one) => (
            map_easing_to_native(one, duration).unwrap_or(NativeEasing::Keyword("linear")),
            None,
        ),
        EaseDefinition::PerSegment(segments) => (
            NativeEasing::Keyword("linear"),
            Some(
                segments
                    .iter()
                    .map(|segment| {
                        // An array's strings are not replaced, and unsupported
                        // names fall back to ease-out.
                        map_easing_to_native(segment, duration)
                            .unwrap_or(NativeEasing::Keyword("ease-out"))
                    })
                    .collect(),
            ),
        ),
    };
    Ok(Outcome::Native(NativeAnimation {
        offsets: options.times,
        keyframe_easings,
        easing,
        delay_ms: options.delay,
        duration_ms: duration,
        iterations: options.repeat + 1.0,
        direction: if options.repeat_type == RepeatType::Reverse {
            Direction::Alternate
        } else {
            Direction::Normal
        },
    }))
}

fn make_instant(options: &mut Options) {
    options.duration = Some(0.0);
    options.kind = TransitionType::Tween;
}
