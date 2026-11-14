//! Reads tests/react_spring/cases.json, the scenarios shared with
//! `tools/react_spring_oracle.py`, and runs them on the port.

use sample_motion::react_spring::{
    preset, ConfigProps, Decay, Easing, Runtime, SpringId, SpringProps, To,
};
use serde_json::Value;
use std::collections::HashMap;
use std::rc::Rc;

pub fn spec() -> Value {
    serde_json::from_str(include_str!("cases.json")).unwrap()
}

/// A number from the JSON.
pub fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| panic!("not a number: {value}"))
}

/// Builds props, keeping one object per preset name as JavaScript's
/// `config.<name>` is one object.
#[derive(Default)]
pub struct Reviver {
    presets: HashMap<String, Rc<ConfigProps>>,
}

impl Reviver {
    pub fn config(&mut self, value: &Value) -> Rc<ConfigProps> {
        if let Some(name) = value.as_str() {
            return Rc::clone(
                self.presets
                    .entry(name.to_owned())
                    .or_insert_with(|| Rc::new(preset(name).expect("a preset"))),
            );
        }
        let object = value.as_object().expect("a config object");
        let mut config = ConfigProps::default();
        for (key, value) in object {
            match key.as_str() {
                "tension" => config.tension = Some(number(value)),
                "friction" => config.friction = Some(number(value)),
                "mass" => config.mass = Some(number(value)),
                "damping" => config.damping = Some(number(value)),
                "frequency" => config.frequency = Some(number(value)),
                "velocity" => config.velocity = Some(number(value)),
                "precision" => config.precision = Some(number(value)),
                "restVelocity" => config.rest_velocity = Some(number(value)),
                "clamp" => config.clamp = Some(value.as_bool().unwrap()),
                "bounce" => config.bounce = Some(number(value)),
                "duration" => config.duration = Some(number(value)),
                "progress" => config.progress = Some(number(value)),
                "round" => config.round = Some(number(value)),
                "easing" => {
                    config.easing =
                        Some(Easing::from_name(value.as_str().unwrap()).expect("an easing"))
                }
                "decay" => {
                    config.decay = Some(match value {
                        Value::Bool(true) => Decay::Default,
                        other => Decay::Rate(number(other)),
                    })
                }
                other => panic!("unknown config key {other}"),
            }
        }
        Rc::new(config)
    }

    pub fn props(&mut self, value: &Value) -> SpringProps {
        let object = value.as_object().expect("a props object");
        let mut props = SpringProps::default();
        for (key, value) in object {
            match key.as_str() {
                "to" => {
                    props.to = Some(match value {
                        Value::Array(steps) => {
                            To::Chain(Rc::new(steps.iter().map(|step| self.props(step)).collect()))
                        }
                        other => To::Value(number(other)),
                    })
                }
                "from" => props.from = Some(number(value)),
                "config" => props.config = Some(self.config(value)),
                "delay" => props.delay = Some(number(value)),
                "immediate" => props.immediate = Some(value.as_bool().unwrap()),
                "reset" => props.reset = Some(value.as_bool().unwrap()),
                "default" => props.default = Some(value.as_bool().unwrap()),
                // Not a react-spring prop: which style the spring drives.
                "target" => {}
                other => panic!("unknown prop {other}"),
            }
        }
        props
    }
}

/// One spring's state after a frame: `get()`, `velocity`, `isAnimating`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub value: f64,
    pub velocity: f64,
    pub animating: bool,
}

/// A case running on the port.
pub struct Run {
    pub runtime: Runtime,
    pub springs: Vec<SpringId>,
    /// The style each spring drives (`"x"` or `"opacity"`).
    pub targets: Vec<String>,
    case: Value,
    reviver: Reviver,
}

impl Run {
    pub fn new(case: &Value) -> Self {
        let mut runtime = Runtime::new();
        let mut reviver = Reviver::default();
        let mut springs = Vec::new();
        let mut targets = Vec::new();
        for spring in case["springs"].as_array().unwrap() {
            springs.push(runtime.create(reviver.props(spring)).unwrap());
            targets.push(spring["target"].as_str().unwrap().to_owned());
        }
        let mut run = Self {
            runtime,
            springs,
            targets,
            case: case.clone(),
            reviver,
        };
        run.actions(0);
        run
    }

    pub fn frames(&self) -> usize {
        self.case["frames"].as_u64().unwrap() as usize
    }

    /// Frame k's time: `k * 1000 / fps`.
    pub fn time(&self, frame: usize) -> f64 {
        let fps = self.case.get("fps").map(number).unwrap_or(60.0);
        frame as f64 * 1000.0 / fps
    }

    /// The actions at `frame`, then a microtask checkpoint.
    fn actions(&mut self, frame: usize) {
        let time = self.time(frame);
        self.runtime.set_time(time);
        let actions = self.case["actions"].as_array().unwrap().clone();
        for action in actions
            .iter()
            .filter(|a| a["frame"].as_u64() == Some(frame as u64))
        {
            let spring = self.springs[action["spring"].as_u64().unwrap() as usize];
            if let Some(props) = action.get("start") {
                let props = self.reviver.props(props);
                self.runtime.start(spring, props).unwrap();
            } else if action.get("stop").is_some() {
                self.runtime.stop(spring);
            }
        }
        self.runtime.perform_microtask_checkpoint();
    }

    /// Run animation frame `frame` (from 1), then its actions.
    pub fn frame(&mut self, frame: usize) {
        let time = self.time(frame);
        self.runtime.animation_frame(time);
        self.actions(frame);
    }

    /// The actions of a frame the caller already ran.
    pub fn actions_after_frame(&mut self, frame: usize) {
        self.actions(frame);
    }

    pub fn samples(&self) -> Vec<Sample> {
        self.springs
            .iter()
            .map(|&spring| Sample {
                value: self.runtime.get(spring).unwrap(),
                velocity: self.runtime.velocity(spring),
                animating: self.runtime.is_animating(spring),
            })
            .collect()
    }
}
