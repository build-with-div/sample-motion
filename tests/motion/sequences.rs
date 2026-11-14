//! Reads tests/motion/sequences.json into the port's types.

use crate::cases;
use sample_motion::motion::{
    AcceleratedValue, At, Delay, Frame, SegmentTransition, SequenceItem, SequenceOptions, Stagger,
    StaggerFrom, Subject, Transition, Value, ValueKeyframes,
};
use serde_json::Value as Json;

pub fn spec() -> Json {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/motion/sequences.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// `animate(elements, keyframes, { ...transition, delay: stagger(...) })`.
pub struct StaggerCase {
    pub name: String,
    pub elements: usize,
    pub value: Value,
    pub transition: Transition,
    pub stagger: Stagger,
}

/// `animate(sequence, options)`.
pub struct SequenceCase {
    pub name: String,
    pub elements: usize,
    pub sequence: Vec<SequenceItem>,
    pub options: SequenceOptions,
}

/// A transition without `delay` and `at`, which are read separately.
fn transition_without(value: &Json, keys: &[&str]) -> Transition {
    let mut value = value.clone();
    if let Some(object) = value.as_object_mut() {
        for key in keys {
            object.remove(*key);
        }
    }
    cases::transition(&value)
}

fn stagger(value: &Json) -> Stagger {
    Stagger {
        duration: cases::number(&value["stagger"]),
        start_delay: value.get("startDelay").map(cases::number).unwrap_or(0.0),
        from: match value.get("from") {
            None => StaggerFrom::Index(0.0),
            Some(Json::Number(index)) => StaggerFrom::Index(index.as_f64().unwrap()),
            Some(Json::String(from)) => match from.as_str() {
                "first" => StaggerFrom::First,
                "last" => StaggerFrom::Last,
                "edges" => StaggerFrom::Edges,
                "center" => StaggerFrom::Center,
                other => panic!("unsupported stagger from {other}"),
            },
            Some(other) => panic!("unsupported stagger from {other}"),
        },
        ease: value.get("ease").map(cases::ease),
        grid: value.get("grid").map(|grid| {
            let grid = grid.as_array().unwrap();
            [cases::number(&grid[0]), cases::number(&grid[1])]
        }),
    }
}

pub fn stagger_cases() -> Vec<StaggerCase> {
    spec()["staggers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            let (property, keyframes) = case["keyframes"]
                .as_object()
                .unwrap()
                .iter()
                .next()
                .unwrap();
            StaggerCase {
                name: case["name"].as_str().unwrap().to_owned(),
                elements: case["elements"].as_u64().unwrap() as usize,
                value: cases::value(property, keyframes),
                transition: transition_without(&case["transition"], &["delay"]),
                stagger: stagger(&case["transition"]["delay"]),
            }
        })
        .collect()
}

fn at(value: &Json) -> At {
    match value {
        Json::String(text) => At::Text(text.clone()),
        other => At::Seconds(cases::number(other)),
    }
}

fn frame(property: AcceleratedValue, value: &Json) -> Frame {
    match property {
        AcceleratedValue::Opacity => Frame::Opacity(cases::number(value)),
        AcceleratedValue::Transform => Frame::Transform(cases::transform(value)),
    }
}

fn item(value: &Json) -> SequenceItem {
    match value {
        Json::String(name) => SequenceItem::Label(name.clone()),
        Json::Object(object) if object.contains_key("sequence") => SequenceItem::Nested {
            sequence: object["sequence"]
                .as_array()
                .unwrap()
                .iter()
                .map(item)
                .collect(),
            at: object.get("at").map(at),
        },
        Json::Object(object) => SequenceItem::LabelAt {
            name: object["name"].as_str().unwrap().to_owned(),
            at: object.get("at").map(at),
        },
        Json::Array(segment) => {
            let subject = match &segment[0] {
                Json::Array(indices) => Subject::Many(
                    indices
                        .iter()
                        .map(|i| i.as_u64().unwrap() as usize)
                        .collect(),
                ),
                index => Subject::One(index.as_u64().unwrap() as usize),
            };
            let values = segment[1]
                .as_object()
                .unwrap()
                .iter()
                .map(|(name, frames)| {
                    let property = match name.as_str() {
                        "opacity" => AcceleratedValue::Opacity,
                        "transform" => AcceleratedValue::Transform,
                        other => panic!("unsupported value {other}"),
                    };
                    let frames = match frames {
                        Json::Array(list) => list.iter().map(|f| frame(property, f)).collect(),
                        single => vec![frame(property, single)],
                    };
                    ValueKeyframes { property, frames }
                })
                .collect();
            let options = segment.get(2).cloned().unwrap_or(Json::Null);
            SequenceItem::Segment {
                subject,
                values,
                transition: Box::new(SegmentTransition {
                    at: options.get("at").map(at),
                    delay: options.get("delay").map(|delay| match delay {
                        Json::Object(_) => Delay::Stagger(stagger(delay)),
                        seconds => Delay::Seconds(cases::number(seconds)),
                    }),
                    transition: transition_without(&options, &["at", "delay"]),
                }),
            }
        }
        other => panic!("unsupported sequence item {other}"),
    }
}

pub fn sequence_cases() -> Vec<SequenceCase> {
    spec()["sequences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            let options = case.get("options").cloned().unwrap_or(Json::Null);
            SequenceCase {
                name: case["name"].as_str().unwrap().to_owned(),
                elements: case["elements"].as_u64().unwrap() as usize,
                sequence: case["sequence"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(item)
                    .collect(),
                options: SequenceOptions {
                    default_transition: options
                        .get("defaultTransition")
                        .map(cases::transition)
                        .unwrap_or_default(),
                    transition: transition_without(&options, &["defaultTransition"]),
                },
            }
        })
        .collect()
}
