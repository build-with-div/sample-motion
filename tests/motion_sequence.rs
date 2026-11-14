//! The port of Motion's `stagger()` and `animate(sequence)` against Motion
//! itself: for every case in tests/motion/sequences.json, each
//! `element.animate()` call the port resolves must equal the call Motion
//! 14.14.0 made on the same element in pinned Chromium, recorded in
//! tests/motion/motion-14.14.0-sequences-oracle.json by
//! `tools/motion_oracle.py sequences`.
//!
//! Strings (easings, including every `linear()` point) and numbers must be
//! equal exactly.

#[allow(dead_code)]
#[path = "motion/cases.rs"]
mod cases;
#[path = "motion/compare.rs"]
mod compare;
#[path = "motion/sequences.rs"]
mod sequences;

use compare::compare;
use sample_motion::motion::{
    create_animations_from_sequence, fill_wildcards, resolve, staggered_transitions,
    AcceleratedValue, Outcome, Transition, Underlying, Value,
};
use serde_json::Value as Json;

fn oracle() -> Json {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/motion/motion-14.14.0-sequences-oracle.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// What a fresh element reads for a leading wildcard, as in the recording.
fn fresh(property: AcceleratedValue) -> Underlying {
    match property {
        AcceleratedValue::Opacity => Underlying::Opacity(1.0),
        AcceleratedValue::Transform => Underlying::NoTransform,
    }
}

fn property_of(value: &Value) -> &'static str {
    match value {
        Value::Opacity(_) => "opacity",
        Value::Transform(_) => "transform",
    }
}

/// Resolve one value on one element and compare it with Motion's call on
/// that element for that property. Returns whether the port called WAAPI.
fn check(
    name: &str,
    element: usize,
    value: &Value,
    transition: &Transition,
    calls: &[Json],
    matched: &mut [bool],
    problems: &mut Vec<String>,
) -> bool {
    let property = property_of(value);
    let label = format!("{name}[{element}].{property}");
    let recorded = calls.iter().enumerate().find(|(_, call)| {
        call["index"].as_u64() == Some(element as u64) && call["keyframes"].get(property).is_some()
    });
    match resolve(value, transition) {
        Ok(Outcome::Native(native)) => match recorded {
            Some((index, call)) => {
                matched[index] = true;
                compare(&label, value, &native, call, problems);
                true
            }
            None => {
                problems.push(format!(
                    "{label}: the port calls element.animate, Motion did not"
                ));
                true
            }
        },
        other => {
            if recorded.is_some() {
                problems.push(format!(
                    "{label}: Motion called element.animate, the port did not ({other:?})"
                ));
            }
            false
        }
    }
}

#[test]
fn port_makes_the_same_element_animate_calls_as_motion_for_stagger_and_sequences() {
    let oracle = oracle();
    assert_eq!(oracle["motion"], "14.14.0");
    let mut problems = Vec::new();
    let mut native_calls = 0;
    let mut case_count = 0;
    let recorded = |name: &str| -> Vec<Json> {
        let case = &oracle["cases"][name];
        assert!(
            case.is_object(),
            "{name}: not in the oracle; rerun tools/motion_oracle.py sequences"
        );
        assert!(
            case["error"].is_null(),
            "{name}: Motion threw {}",
            case["error"]
        );
        case["calls"].as_array().unwrap().clone()
    };
    for case in sequences::stagger_cases() {
        case_count += 1;
        let calls = recorded(&case.name);
        let mut matched = vec![false; calls.len()];
        let transitions = staggered_transitions(&case.transition, &case.stagger, case.elements);
        for (element, transition) in transitions.iter().enumerate() {
            if check(
                &case.name,
                element,
                &case.value,
                transition,
                &calls,
                &mut matched,
                &mut problems,
            ) {
                native_calls += 1;
            }
        }
        if matched.iter().any(|m| !m) {
            problems.push(format!(
                "{}: Motion made calls the port did not match",
                case.name
            ));
        }
    }
    for case in sequences::sequence_cases() {
        case_count += 1;
        let calls = recorded(&case.name);
        let mut matched = vec![false; calls.len()];
        for animation in create_animations_from_sequence(&case.sequence, &case.options) {
            assert!(animation.element < case.elements);
            let frames = fill_wildcards(&animation.frames, &fresh(animation.property)).unwrap();
            let value = Value::from_frames(animation.property, frames).unwrap();
            if check(
                &case.name,
                animation.element,
                &value,
                &animation.transition,
                &calls,
                &mut matched,
                &mut problems,
            ) {
                native_calls += 1;
            }
        }
        if matched.iter().any(|m| !m) {
            problems.push(format!(
                "{}: Motion made calls the port did not match",
                case.name
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "{} differences from Motion:\n{}",
        problems.len(),
        problems.join("\n")
    );
    assert_eq!(case_count, 37);
    assert_eq!(
        native_calls, 111,
        "the oracle records 111 element.animate() calls"
    );
}
