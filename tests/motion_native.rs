//! The Motion port against Motion itself: for every case in
//! tests/motion/cases.json, the `element.animate()` call the port resolves
//! must equal the call Motion 14.14.0 made in pinned Chromium, recorded in
//! tests/motion/motion-14.14.0-oracle.json by `tools/motion_oracle.py values`.
//!
//! Strings (easings, including every `linear()` point) and numbers must be
//! equal exactly.

#[path = "motion/cases.rs"]
mod cases;
#[path = "motion/compare.rs"]
mod compare;

use compare::compare;
use sample_motion::motion::{resolve, Outcome};
use serde_json::Value as Json;

fn oracle() -> Json {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/motion/motion-14.14.0-oracle.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
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
