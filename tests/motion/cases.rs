//! Reads tests/motion/cases.json into the port's types.

use div::prelude::*;
use div::typed_style::{TransformList, TransformOperation};
use sample_motion::motion::{Ease, EaseDefinition, RepeatType, Transition, TransitionType, Value};
use serde_json::Value as Json;

pub struct Case {
    pub name: String,
    pub value: Value,
    pub transition: Transition,
}

pub fn spec() -> Json {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/motion/cases.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

pub fn cases() -> Vec<Case> {
    spec()["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| Case {
            name: case["name"].as_str().unwrap().to_owned(),
            value: value(case["value"].as_str().unwrap(), &case["keyframes"]),
            transition: transition(&case["transition"]),
        })
        .collect()
}

pub fn transform(frame: &Json) -> TransformList {
    TransformList(vec![
        TransformOperation::Translate(
            LengthValue::px(frame["tx"].as_f64().unwrap() as f32),
            LengthValue::px(frame["ty"].as_f64().unwrap() as f32),
        ),
        TransformOperation::Rotate(frame["rotate"].as_f64().unwrap() as f32),
    ])
}

fn value(name: &str, keyframes: &Json) -> Value {
    let frames = keyframes.as_array().unwrap();
    match name {
        "opacity" => Value::Opacity(frames.iter().map(|v| v.as_f64().unwrap() as f32).collect()),
        "transform" => Value::Transform(frames.iter().map(transform).collect()),
        other => panic!("unknown value {other}"),
    }
}

pub fn number(value: &Json) -> f64 {
    match value {
        Json::String(text) if text == "Infinity" => f64::INFINITY,
        other => other.as_f64().unwrap(),
    }
}

fn ease(value: &Json) -> Ease {
    match value {
        Json::String(name) => match name.as_str() {
            "linear" => Ease::Linear,
            "easeIn" => Ease::EaseIn,
            "easeOut" => Ease::EaseOut,
            "easeInOut" => Ease::EaseInOut,
            "circIn" => Ease::CircIn,
            "circOut" => Ease::CircOut,
            "circInOut" => Ease::CircInOut,
            "backIn" => Ease::BackIn,
            "backOut" => Ease::BackOut,
            "backInOut" => Ease::BackInOut,
            "anticipate" => Ease::Anticipate,
            other => panic!("unknown ease {other}"),
        },
        Json::Array(points) => {
            let p: Vec<f64> = points.iter().map(number).collect();
            Ease::Bezier([p[0], p[1], p[2], p[3]])
        }
        other => panic!("unsupported ease {other}"),
    }
}

fn transition(value: &Json) -> Transition {
    let get = |key: &str| value.get(key).map(number);
    Transition {
        kind: value.get("type").map(|kind| match kind.as_str().unwrap() {
            "spring" => TransitionType::Spring,
            "inertia" => TransitionType::Inertia,
            _ => TransitionType::Tween,
        }),
        duration: get("duration"),
        delay: get("delay"),
        ease: value.get("ease").map(|definition| match definition {
            Json::Array(items) if !items.first().is_some_and(Json::is_number) => {
                EaseDefinition::PerSegment(items.iter().map(ease).collect())
            }
            single => EaseDefinition::One(ease(single)),
        }),
        times: value
            .get("times")
            .map(|times| times.as_array().unwrap().iter().map(number).collect()),
        repeat: get("repeat"),
        repeat_type: value
            .get("repeatType")
            .map(|kind| match kind.as_str().unwrap() {
                "reverse" => RepeatType::Reverse,
                "mirror" => RepeatType::Mirror,
                _ => RepeatType::Loop,
            }),
        repeat_delay: get("repeatDelay"),
        stiffness: get("stiffness"),
        damping: get("damping"),
        mass: get("mass"),
        bounce: get("bounce"),
        visual_duration: get("visualDuration"),
        rest_speed: get("restSpeed"),
        rest_delta: get("restDelta"),
    }
}
