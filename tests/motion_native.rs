//! The Motion port against Motion itself: for every case in
//! tests/motion/cases.json, the `element.animate()` call the port resolves
//! must equal the call Motion 14.14.0 made in pinned Chromium, recorded in
//! tests/motion/motion-14.14.0-oracle.json by `tools/motion_oracle.py values`.
//!
//! Strings (easings, including every `linear()` point) and numbers must be
//! equal exactly.

#[path = "motion/cases.rs"]
mod cases;

use sample_motion::motion::{resolve, Direction, NativeAnimation, Outcome, Value};
use serde_json::Value as Json;

fn oracle() -> Json {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/motion/motion-14.14.0-oracle.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
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

fn compare(
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
    let number = |v: &Json| cases::number(v);
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
    let motion_easing = options["easing"].as_str().unwrap_or_default();
    check(
        "easing",
        motion_easing == easing,
        if motion_easing.starts_with("linear(") && easing.starts_with("linear(") {
            first_difference(motion_easing, &easing)
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
            let motion = recorded("transform").unwrap();
            check(
                "keyframes",
                motion.len() == values.len(),
                format!("Motion {} port {}", motion.len(), values.len()),
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
        recorded("easing").map(|e| e.iter().map(|v| v.as_str().unwrap().to_owned()).collect());
    let port_easings: Option<Vec<String>> = native
        .keyframe_easings
        .as_ref()
        .map(|e| e.iter().map(|easing| easing.to_css()).collect());
    check(
        "keyframe easings",
        easings == port_easings,
        format!("Motion {easings:?} port {port_easings:?}"),
    );
}

#[test]
fn port_makes_the_same_element_animate_calls_as_motion() {
    let oracle = oracle();
    assert_eq!(oracle["motion"], "14.14.0");
    let mut problems = Vec::new();
    let mut native_calls = 0;
    let cases = cases::cases();
    for case in &cases {
        let recorded = &oracle["cases"][&case.name];
        assert!(
            recorded.is_object(),
            "{}: not in the oracle; rerun tools/motion_oracle.py values",
            case.name
        );
        assert!(
            recorded["error"].is_null(),
            "{}: Motion threw {}",
            case.name,
            recorded["error"]
        );
        let calls = recorded["calls"].as_array().unwrap();
        match resolve(&case.value, &case.transition) {
            Ok(Outcome::Native(native)) => {
                native_calls += 1;
                if calls.len() != 1 {
                    problems.push(format!(
                        "{}: Motion made {} calls, the port one",
                        case.name,
                        calls.len()
                    ));
                    continue;
                }
                compare(&case.name, &case.value, &native, &calls[0], &mut problems);
            }
            Ok(Outcome::Instant) | Err(_) => {
                if !calls.is_empty() {
                    problems.push(format!(
                        "{}: Motion called element.animate, the port did not ({:?})",
                        case.name,
                        resolve(&case.value, &case.transition)
                    ));
                }
            }
        }
    }
    assert!(
        problems.is_empty(),
        "{} of {} cases differ from Motion:\n{}",
        problems.len(),
        cases.len(),
        problems.join("\n")
    );
    assert_eq!(
        native_calls, 48,
        "the oracle records 48 element.animate() calls"
    );
}
