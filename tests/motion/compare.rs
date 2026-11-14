//! Compares a resolved `element.animate()` call with the one Motion made in
//! Chromium, field by field, exactly.

use div::prelude::*;
use div::typed_style::{TransformList, TransformOperation};
use sample_motion::motion::{Direction, NativeAnimation, Value};
use serde_json::Value as Json;

pub fn number(value: &Json) -> f64 {
    match value {
        Json::String(text) if text == "Infinity" => f64::INFINITY,
        other => other.as_f64().unwrap(),
    }
}

/// The first difference between two `linear(...)` strings, for diagnosis.
fn first_difference(expected: &str, actual: &str) -> String {
    let split = |s: &str| -> Vec<String> {
        s.trim_start_matches("linear(")
            .trim_end_matches(')')
            .split(", ")
            .map(str::to_owned)
            .collect()
    };
    let (e, a) = (split(expected), split(actual));
    for (index, (x, y)) in e.iter().zip(&a).enumerate() {
        if x != y {
            return format!(
                "point {index}: Motion {x}, port {y} ({} vs {} points)",
                e.len(),
                a.len()
            );
        }
    }
    format!("{} vs {} points", e.len(), a.len())
}

/// A transform string Motion passed (`translate(Xpx, Ypx) rotate(Rdeg)`).
pub fn parse_transform(text: &str) -> TransformList {
    let mut operations = Vec::new();
    for function in text.split(')').map(str::trim).filter(|f| !f.is_empty()) {
        let (name, arguments) = function.split_once('(').unwrap();
        let arguments: Vec<&str> = arguments.split(',').map(str::trim).collect();
        let number = |argument: &str, unit: &str| -> f32 {
            argument.strip_suffix(unit).unwrap().parse().unwrap()
        };
        operations.push(match name {
            "translate" => TransformOperation::Translate(
                LengthValue::px(number(arguments[0], "px")),
                LengthValue::px(number(arguments[1], "px")),
            ),
            "rotate" => TransformOperation::Rotate(number(arguments[0], "deg")),
            other => panic!("unexpected transform function {other}"),
        });
    }
    TransformList(operations)
}

fn easing_text(easing: &Json) -> String {
    easing.as_str().unwrap_or_default().to_owned()
}

pub fn compare(
    name: &str,
    value: &Value,
    native: &NativeAnimation,
    call: &Json,
    problems: &mut Vec<String>,
) {
    let options = &call["options"];
    let mut check = |field: &str, ok: bool, detail: String| {
        if !ok {
            problems.push(format!("{name}: {field}: {detail}"));
        }
    };
    check(
        "delay",
        number(&options["delay"]) == native.delay_ms,
        format!("Motion {} port {}", options["delay"], native.delay_ms),
    );
    check(
        "duration",
        number(&options["duration"]) == native.duration_ms,
        format!("Motion {} port {}", options["duration"], native.duration_ms),
    );
    let easing = native.easing.to_css();
    let motion_easing = easing_text(&options["easing"]);
    check(
        "easing",
        motion_easing == easing,
        if motion_easing.starts_with("linear(") && easing.starts_with("linear(") {
            first_difference(&motion_easing, &easing)
        } else {
            format!("Motion {motion_easing} port {easing}")
        },
    );
    check(
        "fill",
        options["fill"] == "both",
        format!("{}", options["fill"]),
    );
    check(
        "iterations",
        number(&options["iterations"]) == native.iterations,
        format!(
            "Motion {} port {}",
            options["iterations"], native.iterations
        ),
    );
    let direction = match native.direction {
        Direction::Normal => "normal",
        Direction::Alternate => "alternate",
    };
    check(
        "direction",
        options["direction"] == direction,
        format!("Motion {} port {direction}", options["direction"]),
    );
    let keyframes = &call["keyframes"];
    let recorded = |key: &str| keyframes.get(key).and_then(Json::as_array).cloned();
    match value {
        Value::Opacity(values) => {
            let motion: Vec<f64> = recorded("opacity").unwrap().iter().map(number).collect();
            let port: Vec<f64> = values.iter().map(|v| f64::from(*v)).collect();
            // f32 keyframes: compare at f32 precision.
            let same = motion.len() == port.len()
                && motion
                    .iter()
                    .zip(&port)
                    .all(|(m, p)| (*m as f32) == (*p as f32));
            check(
                "keyframes",
                same,
                format!("Motion {motion:?} port {port:?}"),
            );
        }
        Value::Transform(values) => {
            let motion: Vec<TransformList> = recorded("transform")
                .unwrap()
                .iter()
                .map(|v| parse_transform(v.as_str().unwrap()))
                .collect();
            check(
                "keyframes",
                &motion == values,
                format!("Motion {motion:?} port {values:?}"),
            );
        }
    }
    let offsets: Option<Vec<f64>> = recorded("offset").map(|o| o.iter().map(number).collect());
    check(
        "offset",
        offsets == native.offsets,
        format!("Motion {offsets:?} port {:?}", native.offsets),
    );
    let easings: Option<Vec<String>> =
        recorded("easing").map(|e| e.iter().map(easing_text).collect());
    let port_easings: Option<Vec<String>> = native
        .keyframe_easings
        .as_ref()
        .map(|e| e.iter().map(|easing| easing.to_css()).collect());
    if let (Some(motion), Some(port)) = (&easings, &port_easings) {
        let diffs: Vec<String> = motion
            .iter()
            .zip(port)
            .enumerate()
            .filter(|(_, (m, p))| m != p)
            .map(|(i, (m, p))| {
                if m.starts_with("linear(") && p.starts_with("linear(") {
                    format!("[{i}] {}", first_difference(m, p))
                } else {
                    format!("[{i}] Motion {m} port {p}")
                }
            })
            .collect();
        check(
            "keyframe easings",
            motion.len() == port.len() && diffs.is_empty(),
            format!(
                "{} vs {} entries; {}",
                motion.len(),
                port.len(),
                diffs.join("; ")
            ),
        );
    } else {
        check(
            "keyframe easings",
            easings == port_easings,
            format!("Motion {easings:?} port {port_easings:?}"),
        );
    }
}
