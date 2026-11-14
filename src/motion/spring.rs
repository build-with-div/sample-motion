//! Motion's spring generator, ported from motion-dom 14.14.0
//! `animation/generators/spring.mjs` with the same formulas and operation
//! order. Times are milliseconds, as in Motion.

use super::js::{clamp, milliseconds_to_seconds, seconds_to_milliseconds};

const DEFAULT_STIFFNESS: f64 = 100.0;
const DEFAULT_DAMPING: f64 = 10.0;
const DEFAULT_MASS: f64 = 1.0;
const DEFAULT_DURATION_MS: f64 = 800.0;
const DEFAULT_BOUNCE: f64 = 0.3;
const REST_SPEED_GRANULAR: f64 = 0.01;
const REST_SPEED_DEFAULT: f64 = 2.0;
const REST_DELTA_GRANULAR: f64 = 0.005;
const REST_DELTA_DEFAULT: f64 = 0.5;
const MIN_DURATION_S: f64 = 0.01;
const MAX_DURATION_S: f64 = 10.0;
const MIN_DAMPING: f64 = 0.05;
const ROOT_ITERATIONS: usize = 12;
const SAFE_MIN: f64 = 0.001;

/// The spring options of a Motion transition. `None` is JavaScript's
/// `undefined`. Durations are milliseconds and `visual_duration` is seconds,
/// as Motion receives them by the time the generator runs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpringOptions {
    pub stiffness: Option<f64>,
    pub damping: Option<f64>,
    pub mass: Option<f64>,
    pub duration: Option<f64>,
    pub bounce: Option<f64>,
    pub visual_duration: Option<f64>,
    pub velocity: Option<f64>,
    pub rest_speed: Option<f64>,
    pub rest_delta: Option<f64>,
}

fn bounce_to_damping_ratio(bounce: f64) -> f64 {
    if bounce < 0.0 {
        1.0 / (1.0 + bounce).max(MIN_DAMPING)
    } else {
        (1.0 - bounce).max(MIN_DAMPING)
    }
}

fn approximate_root(
    envelope: impl Fn(f64) -> f64,
    derivative: impl Fn(f64) -> f64,
    guess: f64,
) -> f64 {
    let mut result = guess;
    for _ in 1..ROOT_ITERATIONS {
        result -= envelope(result) / derivative(result);
    }
    result
}

fn overdamped_freq_scale(damping_ratio: f64, critical_freq_time: f64) -> f64 {
    // `!(dampingRatio > 1)` is also true for NaN.
    if damping_ratio.is_nan() || damping_ratio <= 1.0 {
        return 1.0;
    }
    let root = (damping_ratio * damping_ratio - 1.0).sqrt();
    let slow = damping_ratio - root;
    let fast = damping_ratio + root;
    let target = 2.0 * root * (-critical_freq_time).exp() * (1.0 + critical_freq_time);
    approximate_root(
        |t| fast * (-slow * t).exp() - slow * (-fast * t).exp() - target,
        |t| (-fast * t).exp() - (-slow * t).exp(),
        critical_freq_time,
    ) / critical_freq_time
}

fn calc_angular_freq(undamped_freq: f64, damping_ratio: f64) -> f64 {
    undamped_freq * (1.0 - damping_ratio * damping_ratio).sqrt()
}

struct Physics {
    stiffness: f64,
    damping: f64,
    duration: f64,
}

/// `findSpring`: duration- and bounce-based resolution.
fn find_spring(duration: Option<f64>, bounce: Option<f64>) -> Physics {
    let damping_ratio = bounce_to_damping_ratio(bounce.unwrap_or(DEFAULT_BOUNCE));
    let duration = clamp(
        MIN_DURATION_S,
        MAX_DURATION_S,
        milliseconds_to_seconds(duration.unwrap_or(DEFAULT_DURATION_MS)),
    );
    let critical_freq = if damping_ratio < 1.0 {
        let envelope = move |undamped_freq: f64| {
            let exponential_decay = undamped_freq * damping_ratio;
            let delta = exponential_decay * duration;
            let b = calc_angular_freq(undamped_freq, damping_ratio);
            let c = (-delta).exp();
            SAFE_MIN - (exponential_decay / b) * c
        };
        let derivative = move |undamped_freq: f64| {
            let exponential_decay = undamped_freq * damping_ratio;
            let delta = exponential_decay * duration;
            let e = damping_ratio * damping_ratio * undamped_freq * undamped_freq * duration;
            let f = (-delta).exp();
            let g = calc_angular_freq(undamped_freq * undamped_freq, damping_ratio);
            let factor = if -envelope(undamped_freq) + SAFE_MIN > 0.0 {
                -1.0
            } else {
                1.0
            };
            (factor * -e * f) / g
        };
        approximate_root(envelope, derivative, 5.0 / duration)
    } else {
        let envelope = move |undamped_freq: f64| {
            let a = (-undamped_freq * duration).exp();
            let b = undamped_freq * duration + 1.0;
            -SAFE_MIN + a * b
        };
        let derivative = move |undamped_freq: f64| {
            let a = (-undamped_freq * duration).exp();
            let b = -undamped_freq * (duration * duration);
            a * b
        };
        approximate_root(envelope, derivative, 5.0 / duration)
    };
    let undamped_freq =
        critical_freq * overdamped_freq_scale(damping_ratio, critical_freq * duration);
    let stiffness = undamped_freq * undamped_freq;
    Physics {
        stiffness,
        damping: damping_ratio * 2.0 * stiffness.sqrt(),
        duration: seconds_to_milliseconds(duration),
    }
}

fn is_valid_physics(value: f64, can_be_zero: bool) -> bool {
    (if can_be_zero {
        value >= 0.0
    } else {
        value > 0.0
    }) && value < f64::INFINITY
}

fn resolve_physics(value: Option<f64>, can_be_zero: bool) -> Option<f64> {
    value.filter(|v| is_valid_physics(*v, can_be_zero))
}

/// `getSpringOptions`.
struct Resolved {
    stiffness: f64,
    damping: f64,
    mass: f64,
    duration: Option<f64>,
    is_resolved_from_duration: bool,
    is_time_defined: bool,
}

fn get_spring_options(options: &SpringOptions) -> Resolved {
    let valid_stiffness = resolve_physics(options.stiffness, false);
    let valid_damping = resolve_physics(options.damping, true);
    let valid_mass = resolve_physics(options.mass, false);
    let mut resolved = Resolved {
        stiffness: valid_stiffness.unwrap_or(DEFAULT_STIFFNESS),
        damping: valid_damping.unwrap_or(DEFAULT_DAMPING),
        mass: valid_mass.unwrap_or(DEFAULT_MASS),
        duration: options.duration,
        is_resolved_from_duration: false,
        is_time_defined: valid_stiffness.or(valid_damping).or(valid_mass).is_none()
            && (options.duration.is_some() || options.bounce.is_some()),
    };
    if resolved.is_time_defined {
        // `if (options.visualDuration)`: zero and NaN are falsy.
        match options.visual_duration.filter(|v| *v != 0.0 && !v.is_nan()) {
            Some(visual_duration) => {
                // `options.bounce || 0`
                let bounce = options
                    .bounce
                    .filter(|b| *b != 0.0 && !b.is_nan())
                    .unwrap_or(0.0);
                let damping_ratio = bounce_to_damping_ratio(bounce);
                let root = ((2.0 * std::f64::consts::PI) / (visual_duration * 1.2))
                    * overdamped_freq_scale(damping_ratio, (2.0 * std::f64::consts::PI) / 1.2);
                resolved.stiffness = root * root;
                resolved.damping = 2.0 * damping_ratio * resolved.stiffness.sqrt();
            }
            None => {
                let physics = find_spring(options.duration, options.bounce);
                resolved.stiffness = physics.stiffness;
                resolved.damping = physics.damping;
                resolved.duration = Some(physics.duration);
                resolved.is_resolved_from_duration = true;
            }
        }
        if !is_valid_physics(resolved.stiffness, false) || !is_valid_physics(resolved.damping, true)
        {
            resolved.stiffness = DEFAULT_STIFFNESS;
            resolved.damping = DEFAULT_DAMPING;
        }
    }
    resolved
}

enum Curve {
    Underdamped {
        angular_freq: f64,
        a: f64,
        sin_c: f64,
        cos_c: f64,
    },
    Critical {
        c: f64,
    },
    Overdamped {
        slow: f64,
        fast: f64,
        limit: f64,
        s: f64,
        f: f64,
    },
}

/// A spring from `keyframes[0]` to the last keyframe, sampled with
/// [`Spring::next`].
pub struct Spring {
    target: f64,
    delta: f64,
    velocity: f64,
    rest_speed: f64,
    rest_delta: f64,
    undamped_angular_freq: f64,
    decay: f64,
    curve: Curve,
    duration: Option<f64>,
    is_resolved_from_duration: bool,
    /// `generator.calculatedDuration`.
    pub calculated_duration: Option<f64>,
}

impl Spring {
    /// `spring({ ...options, keyframes: [origin, target] })`.
    pub fn new(options: &SpringOptions, origin: f64, target: f64) -> Self {
        let resolved = get_spring_options(options);
        let inherit_velocity = |velocity: f64| {
            if resolved.is_resolved_from_duration {
                0.0
            } else {
                -milliseconds_to_seconds(velocity)
            }
        };
        // `inheritVelocity(options.velocity) || 0`: NaN, -0 and undefined give 0.
        let velocity = options
            .velocity
            .map(inherit_velocity)
            .filter(|v| *v != 0.0 && !v.is_nan())
            .unwrap_or(0.0);
        let delta = target - origin;
        let granular = delta.abs() < 5.0;
        // `options.restSpeed || default`
        let truthy = |v: Option<f64>| v.filter(|v| *v != 0.0 && !v.is_nan());
        let rest_speed = truthy(options.rest_speed).unwrap_or(if granular {
            REST_SPEED_GRANULAR
        } else {
            REST_SPEED_DEFAULT
        });
        let rest_delta = truthy(options.rest_delta).unwrap_or(if granular {
            REST_DELTA_GRANULAR
        } else {
            REST_DELTA_DEFAULT
        });
        let Resolved {
            stiffness,
            damping,
            mass,
            ..
        } = resolved;
        let damping_ratio = damping / (2.0 * (stiffness * mass).sqrt());
        let undamped_angular_freq = milliseconds_to_seconds((stiffness / mass).sqrt());
        let decay = damping_ratio * undamped_angular_freq;
        let curve = if damping_ratio < 1.0 {
            let angular_freq = calc_angular_freq(undamped_angular_freq, damping_ratio);
            let a = (velocity + decay * delta) / angular_freq;
            Curve::Underdamped {
                angular_freq,
                a,
                sin_c: decay * a + delta * angular_freq,
                cos_c: decay * delta - a * angular_freq,
            }
        } else if damping_ratio == 1.0 {
            Curve::Critical {
                c: velocity + undamped_angular_freq * delta,
            }
        } else {
            let damped_angular_freq =
                undamped_angular_freq * (damping_ratio * damping_ratio - 1.0).sqrt();
            let limit = if damped_angular_freq < 1.0 || resolved.is_time_defined {
                f64::INFINITY
            } else {
                300.0 / damped_angular_freq
            };
            let p = (velocity + decay * delta) / damped_angular_freq;
            Curve::Overdamped {
                slow: decay - damped_angular_freq,
                fast: decay + damped_angular_freq,
                limit,
                s: (delta + p) / 2.0,
                f: (delta - p) / 2.0,
            }
        };
        let calculated_duration = if resolved.is_resolved_from_duration {
            resolved.duration.filter(|d| *d != 0.0 && !d.is_nan())
        } else {
            None
        };
        Self {
            target,
            delta,
            velocity,
            rest_speed,
            rest_delta,
            undamped_angular_freq,
            decay,
            curve,
            duration: resolved.duration,
            is_resolved_from_duration: resolved.is_resolved_from_duration,
            calculated_duration,
        }
    }

    fn decay_at(&self, rate: f64, limit: f64, t: f64) -> f64 {
        if t > limit {
            ((self.decay - rate) * limit - self.decay * t).exp()
        } else {
            (-rate * t).exp()
        }
    }

    fn resolve_spring(&self, t: f64) -> f64 {
        match self.curve {
            Curve::Underdamped {
                angular_freq, a, ..
            } => {
                let env = (-self.decay * t).exp();
                self.target
                    - env * (a * (angular_freq * t).sin() + self.delta * (angular_freq * t).cos())
            }
            Curve::Critical { .. } => {
                let w = self.undamped_angular_freq;
                self.target - (-w * t).exp() * (self.delta + (self.velocity + w * self.delta) * t)
            }
            Curve::Overdamped {
                slow,
                fast,
                limit,
                s,
                f,
            } => {
                self.target - s * self.decay_at(slow, limit, t) - f * self.decay_at(fast, limit, t)
            }
        }
    }

    fn resolve_velocity(&self, t: f64) -> f64 {
        match self.curve {
            Curve::Underdamped {
                angular_freq,
                sin_c,
                cos_c,
                ..
            } => {
                let env = (-self.decay * t).exp();
                env * (sin_c * (angular_freq * t).sin() + cos_c * (angular_freq * t).cos())
            }
            Curve::Critical { c } => {
                let w = self.undamped_angular_freq;
                (-w * t).exp() * (w * c * t - self.velocity)
            }
            Curve::Overdamped {
                slow,
                fast,
                limit,
                s,
                f,
            } => {
                slow * s * self.decay_at(slow, limit, t) + fast * f * self.decay_at(fast, limit, t)
            }
        }
    }

    /// `generator.next(t)`: the value at `t` ms and whether it has settled.
    pub fn next(&self, t: f64) -> (f64, bool) {
        let current = self.resolve_spring(t);
        let done = if !self.is_resolved_from_duration {
            let current_velocity = seconds_to_milliseconds(self.resolve_velocity(t));
            current_velocity.abs() <= self.rest_speed
                && (self.target - current).abs() <= self.rest_delta
        } else {
            // `t >= duration`, with an undefined duration comparing false.
            self.duration.is_some_and(|d| t >= d)
        };
        (if done { self.target } else { current }, done)
    }
}
