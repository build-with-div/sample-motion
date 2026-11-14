//! The port's per-frame values against react-spring 10.1.2 in Chromium.
//!
//! Each case of tests/react_spring/cases.json runs on the port at the same
//! frame times the oracle pumped, and every spring's `get()`, `velocity`
//! and `isAnimating` after every frame must equal the values recorded by
//! `tools/react_spring_oracle.py values` (bit for bit).

#[allow(dead_code)]
#[path = "react_spring/cases.rs"]
mod cases;

use cases::{number, Run, Sample};
use serde_json::Value;

fn oracle() -> Value {
    serde_json::from_str(include_str!("react_spring/react-spring-10.1.2-oracle.json")).unwrap()
}

fn recorded(value: &Value) -> Sample {
    let sample = value.as_array().unwrap();
    Sample {
        value: number(&sample[0]),
        velocity: number(&sample[1]),
        animating: sample[2].as_bool().unwrap(),
    }
}

fn same(a: &Sample, b: &Sample) -> bool {
    a.value.to_bits() == b.value.to_bits()
        && a.velocity.to_bits() == b.velocity.to_bits()
        && a.animating == b.animating
}

#[test]
fn values_equal_react_spring() {
    let spec = cases::spec();
    let oracle = oracle();
    let cases = spec["cases"].as_array().unwrap();
    let mut failures = Vec::new();
    let mut frames = 0;
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let expected = oracle["cases"][name]["frames"]
            .as_array()
            .unwrap_or_else(|| panic!("{name}: not in the oracle"));
        let mut run = Run::new(case);
        assert_eq!(expected.len(), run.frames() + 1, "{name}: frame count");
        for (frame, expected) in expected.iter().enumerate() {
            if frame > 0 {
                run.frame(frame);
            }
            let expected: Vec<Sample> = expected.as_array().unwrap().iter().map(recorded).collect();
            let actual = run.samples();
            frames += 1;
            if expected.len() != actual.len()
                || expected.iter().zip(&actual).any(|(a, b)| !same(a, b))
            {
                failures.push(format!(
                    "{name} frame {frame}: expected {expected:?}, got {actual:?}"
                ));
            }
        }
    }
    let shown: Vec<_> = failures.iter().take(20).collect();
    assert!(
        failures.is_empty(),
        "{} of {frames} frames differ:\n{shown:#?}",
        failures.len()
    );
    assert_eq!(cases.len(), 49);
}
