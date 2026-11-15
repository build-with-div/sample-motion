//! The port's effects against AutoAnimate 0.10.2 in Chromium.
//!
//! Each case of tests/auto_animate/cases.json runs on the port at the
//! oracle's frame times. What AutoAnimate does in each frame, in order,
//! must equal what `tools/auto_animate_oracle.py values` recorded: its
//! `element.animate()` calls with their keyframes and options, its
//! `cancel()` and `finish()` calls, the animations' ends, its style
//! assignments and its DOM operations. Div must accept every write except
//! `transform-origin: center` (F24). A case marked with a `gap` is expected
//! to differ until Div closes that gap (docs/framework-gaps.md).

#[allow(dead_code)]
#[path = "auto_animate/cases.rs"]
mod cases;

use cases::Run;
use serde_json::Value;

fn oracle() -> Value {
    serde_json::from_str(include_str!("auto_animate/auto-animate-0.10.2-oracle.json")).unwrap()
}

fn text(events: &[Value]) -> String {
    serde_json::to_string(events).unwrap()
}

#[test]
fn effects_equal_auto_animate() {
    let spec = cases::spec();
    let oracle = oracle();
    let cases = spec["cases"].as_array().unwrap();
    let mut failures = Vec::new();
    let mut gap_cases = Vec::new();
    let (mut frames, mut events, mut origin_errors) = (0, 0, 0);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let expected = oracle["cases"][name]["frames"]
            .as_array()
            .unwrap_or_else(|| panic!("{name}: not in the oracle"));
        let (mut run, first) = Run::new(&spec, case);
        assert_eq!(expected.len(), run.frames() + 1, "{name}: frame count");
        let mut got = vec![first];
        for k in 1..=run.frames() {
            got.push(run.frame(k));
        }
        let mut differing = Vec::new();
        for (k, (want, got)) in expected.iter().zip(&got).enumerate() {
            let want = text(want.as_array().unwrap());
            let got = text(got);
            if want != got {
                differing.push((k, want, got));
            }
        }
        for error in run.aa.errors() {
            if error.starts_with("transform-origin: \"center\"") {
                origin_errors += 1;
            } else {
                failures.push(format!("{name}: Div error: {error}"));
            }
        }
        if let Some(gap) = case["gap"].as_str() {
            gap_cases.push(name.to_string());
            assert!(
                !differing.is_empty(),
                "{name}: gap {gap} no longer changes the result; update the case and \
                 docs/framework-gaps.md"
            );
            let (k, want, got) = &differing[0];
            println!(
                "{name} ({gap}): {} of {} frames differ; first at frame {k}:\n  \
                 Chromium {want}\n  port     {got}",
                differing.len(),
                expected.len()
            );
            continue;
        }
        frames += expected.len();
        events += expected
            .iter()
            .map(|f| f.as_array().unwrap().len())
            .sum::<usize>();
        for (k, want, got) in differing.iter().take(3) {
            failures.push(format!(
                "{name} frame {k}:\n  Chromium {want}\n  port     {got}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(cases.len(), 26);
    assert_eq!(
        gap_cases,
        ["gap-f8-remove-readd", "gap-f9-reduced-motion-disrespect"]
    );
    println!(
        "{} cases, {frames} frames, {events} events equal; {origin_errors} \
         transform-origin writes Div could not parse (F24)",
        cases.len() - gap_cases.len()
    );
}
