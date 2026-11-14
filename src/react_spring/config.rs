//! Spring configuration (`@react-spring/core` 10.1.2 `constants.ts`,
//! `AnimationConfig.ts`) and the easings it accepts (`@react-spring/shared`
//! `easings.ts`).

/// A `config` prop. `None` is an absent key.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConfigProps {
    pub tension: Option<f64>,
    pub friction: Option<f64>,
    pub mass: Option<f64>,
    pub damping: Option<f64>,
    pub frequency: Option<f64>,
    pub velocity: Option<f64>,
    pub precision: Option<f64>,
    pub rest_velocity: Option<f64>,
    pub clamp: Option<bool>,
    pub bounce: Option<f64>,
    pub duration: Option<f64>,
    pub easing: Option<Easing>,
    pub progress: Option<f64>,
    pub decay: Option<Decay>,
    pub round: Option<f64>,
}

/// `config.decay`: `true` (0.998) or a rate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Decay {
    Default,
    Rate(f64),
}

/// The named presets (`config.default`, `config.gentle`, ...).
pub fn preset(name: &str) -> Option<ConfigProps> {
    let (tension, friction) = match name {
        "default" => (170.0, 26.0),
        "gentle" => (120.0, 14.0),
        "wobbly" => (180.0, 12.0),
        "stiff" => (210.0, 20.0),
        "slow" => (280.0, 60.0),
        "molasses" => (280.0, 120.0),
        _ => return None,
    };
    Some(ConfigProps {
        tension: Some(tension),
        friction: Some(friction),
        ..ConfigProps::default()
    })
}

/// The easings react-spring exports that the port covers: those built from
/// arithmetic and `Math.pow` only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    Linear,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutQuad,
    EaseInCubic,
    EaseOutCubic,
    EaseInOutCubic,
    EaseInQuart,
    EaseOutQuart,
    EaseInBack,
    EaseOutBack,
}

const C1: f64 = 1.70158;
const C3: f64 = C1 + 1.0;

impl Easing {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "linear" => Self::Linear,
            "easeInQuad" => Self::EaseInQuad,
            "easeOutQuad" => Self::EaseOutQuad,
            "easeInOutQuad" => Self::EaseInOutQuad,
            "easeInCubic" => Self::EaseInCubic,
            "easeOutCubic" => Self::EaseOutCubic,
            "easeInOutCubic" => Self::EaseInOutCubic,
            "easeInQuart" => Self::EaseInQuart,
            "easeOutQuart" => Self::EaseOutQuart,
            "easeInBack" => Self::EaseInBack,
            "easeOutBack" => Self::EaseOutBack,
            _ => return None,
        })
    }

    pub fn apply(self, x: f64) -> f64 {
        match self {
            Self::Linear => x,
            Self::EaseInQuad => x * x,
            Self::EaseOutQuad => 1.0 - (1.0 - x) * (1.0 - x),
            Self::EaseInOutQuad => {
                if x < 0.5 {
                    2.0 * x * x
                } else {
                    1.0 - (-2.0 * x + 2.0).powf(2.0) / 2.0
                }
            }
            Self::EaseInCubic => x * x * x,
            Self::EaseOutCubic => 1.0 - (1.0 - x).powf(3.0),
            Self::EaseInOutCubic => {
                if x < 0.5 {
                    4.0 * x * x * x
                } else {
                    1.0 - (-2.0 * x + 2.0).powf(3.0) / 2.0
                }
            }
            Self::EaseInQuart => x * x * x * x,
            Self::EaseOutQuart => 1.0 - (1.0 - x).powf(4.0),
            Self::EaseInBack => C3 * x * x * x - C1 * x * x,
            Self::EaseOutBack => 1.0 + C3 * (x - 1.0).powf(3.0) + C1 * (x - 1.0).powf(2.0),
        }
    }
}

/// An animation's resolved config (`AnimationConfig` after `mergeConfig`).
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationConfig {
    pub tension: f64,
    pub friction: f64,
    pub mass: f64,
    pub damping: f64,
    pub frequency: Option<f64>,
    pub velocity: f64,
    pub precision: Option<f64>,
    pub rest_velocity: Option<f64>,
    pub clamp: bool,
    pub bounce: Option<f64>,
    pub duration: Option<f64>,
    pub easing: Easing,
    pub progress: Option<f64>,
    pub decay: Option<Decay>,
    pub round: Option<f64>,
}

impl Default for AnimationConfig {
    /// `new AnimationConfig()`: velocity 0 and `defaults`
    /// (`config.default`, mass 1, damping 1, linear, no clamp).
    fn default() -> Self {
        Self {
            tension: 170.0,
            friction: 26.0,
            mass: 1.0,
            damping: 1.0,
            frequency: None,
            velocity: 0.0,
            precision: None,
            rest_velocity: None,
            clamp: false,
            bounce: None,
            duration: None,
            easing: Easing::Linear,
            progress: None,
            decay: None,
            round: None,
        }
    }
}

/// `sanitizeConfig(config, props)`: a duration and a decay give way to
/// physics props, and tension/friction to frequency.
fn sanitize(config: &mut AnimationConfig, props: &ConfigProps) {
    if props.decay.is_some() {
        config.duration = None;
    } else {
        let tension_config = props.tension.is_some() || props.friction.is_some();
        if tension_config
            || props.frequency.is_some()
            || props.damping.is_some()
            || props.mass.is_some()
        {
            config.duration = None;
            config.decay = None;
        }
        if tension_config {
            config.frequency = None;
        }
    }
}

/// `{ ...base, ...top }` for config props.
fn spread(base: &ConfigProps, top: &ConfigProps) -> ConfigProps {
    macro_rules! pick {
        ($($field:ident),*) => {
            ConfigProps { $($field: top.$field.or(base.$field)),* }
        };
    }
    pick!(
        tension,
        friction,
        mass,
        damping,
        frequency,
        velocity,
        precision,
        rest_velocity,
        clamp,
        bounce,
        duration,
        easing,
        progress,
        decay,
        round
    )
}

/// `mergeConfig(config, newConfig, defaultConfig)`.
pub fn merge_config(
    config: &mut AnimationConfig,
    new_config: &ConfigProps,
    default_config: Option<&ConfigProps>,
) {
    let mut new_config = new_config.clone();
    if let Some(default_config) = default_config {
        // `sanitizeConfig(defaultConfig, newConfig)` acts on a copy of the
        // default props: drop what the new props override.
        let mut defaults = default_config.clone();
        if new_config.decay.is_some() {
            defaults.duration = None;
        } else {
            let tension_config = new_config.tension.is_some() || new_config.friction.is_some();
            if tension_config
                || new_config.frequency.is_some()
                || new_config.damping.is_some()
                || new_config.mass.is_some()
            {
                defaults.duration = None;
                defaults.decay = None;
            }
            if tension_config {
                defaults.frequency = None;
            }
        }
        new_config = spread(&defaults, &new_config);
    }
    sanitize(config, &new_config);
    // `Object.assign(config, newConfig)`, then `defaults` for null keys
    // (tension, friction, mass, damping, easing, clamp always have values).
    macro_rules! assign {
        ($($field:ident),*) => {$(
            if let Some(value) = new_config.$field {
                config.$field = value;
            }
        )*};
    }
    assign!(tension, friction, mass, damping, velocity, clamp, easing);
    macro_rules! assign_option {
        ($($field:ident),*) => {$(
            if new_config.$field.is_some() {
                config.$field = new_config.$field;
            }
        )*};
    }
    assign_option!(
        frequency,
        precision,
        rest_velocity,
        bounce,
        duration,
        progress,
        decay,
        round
    );
    if let Some(mut frequency) = config.frequency {
        let mass = config.mass;
        let mut damping = config.damping;
        if frequency < 0.01 {
            frequency = 0.01;
        }
        if damping < 0.0 {
            damping = 0.0;
        }
        config.tension = (2.0 * std::f64::consts::PI / frequency).powf(2.0) * mass;
        config.friction = 4.0 * std::f64::consts::PI * damping * mass / frequency;
    }
}
