//! The port's style writes against anime.js 4.5.0 in Chromium.
//!
//! Each case of tests/anime/cases.json runs on the port at the same frame
//! times and `Date.now()` the oracle used, and every `el.style.opacity` and
//! `el.style.transform` assignment of every frame, in order, must equal the
//! strings `tools/anime_oracle.py values` recorded. Div must accept every
//! written string. A case marked with a `gap` is expected to differ until
//! Div closes that gap (docs/framework-gaps.md).

#[allow(dead_code)]
#[path = "anime/cases.rs"]
mod cases;

use cases::Run;
use sample_motion::anime::StyleWrite;
use serde_json::Value;

fn oracle() -> Value {
    serde_json::from_str(include_str!("anime/animejs-4.5.0-oracle.json")).unwrap()
}

fn recorded(frame: &Value) -> Vec<(usize, String, String)> {
    frame
        .as_array()
        .unwrap()
        .iter()
        .map(|w| {
            let w = w.as_array().unwrap();
            (
                w[0].as_u64().unwrap() as usize,
                w[1].as_str().unwrap().to_string(),
                w[2].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn ported(writes: &[StyleWrite]) -> Vec<(usize, String, String)> {
    writes
        .iter()
        .map(|w| (w.target.0, w.property.clone(), w.value.clone()))
        .collect()
}

#[test]
fn writes_equal_animejs() {
    let spec = cases::spec();
    let oracle = oracle();
    let boxes = spec["boxes"].as_u64().unwrap() as usize;
    let cases = spec["cases"].as_array().unwrap();
    let mut failures = Vec::new();
    let mut gap_cases = Vec::new();
    let (mut frames, mut writes) = (0, 0);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let expected = oracle["cases"][name]["frames"]
            .as_array()
            .unwrap_or_else(|| panic!("{name}: not in the oracle"));
        let (mut run, first) = Run::new(case, boxes);
        assert_eq!(expected.len(), run.frames() + 1, "{name}: frame count");
        let mut differing = Vec::new();
        let (mut case_frames, mut case_writes) = (0, 0);
        for (frame, expected) in expected.iter().enumerate() {
            let actual = if frame == 0 {
                first.clone()
            } else {
                run.frame(frame)
            };
            let (expected, actual) = (recorded(expected), ported(&actual));
            case_frames += 1;
            case_writes += expected.len();
            if expected != actual {
                differing.push(format!(
                    "{name} frame {frame}: expected {expected:?}, got {actual:?}"
                ));
            }
        }
        assert!(
            run.runtime.host_errors().is_empty(),
            "{name}: Div rejected writes: {:?}",
            run.runtime.host_errors()
        );
        match case.get("gap") {
            Some(gap) => {
                assert!(
                    !differing.is_empty(),
                    "{name} equals anime.js: gap {gap} may be closed; remove the mark"
                );
                println!(
                    "{name} (gap {gap}): {} of {case_frames} frames differ, first: {}",
                    differing.len(),
                    differing[0]
                );
                gap_cases.push(name);
            }
            None => {
                failures.extend(differing);
                frames += case_frames;
                writes += case_writes;
            }
        }
    }
    let shown: Vec<_> = failures.iter().take(20).collect();
    assert!(
        failures.is_empty(),
        "{} of {frames} frames differ:\n{shown:#?}",
        failures.len()
    );
    assert_eq!(cases.len(), 47);
    assert_eq!(gap_cases, ["gap-f19-readback"]);
    println!(
        "{} cases without a gap: {frames} frames, {writes} writes equal",
        cases.len() - gap_cases.len()
    );
}
