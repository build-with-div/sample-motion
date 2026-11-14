//! Div's side of the Motion pixel comparison: the port animates the same
//! box as `tools/motion_oracle.py pixels` does with Motion in Chromium, and
//! writes `out/motion/div/<case>-<t>.png` on Div's headless clock.
//!
//! ```text
//! cargo test --test motion_pixels -- --ignored
//! ```

#[allow(dead_code)]
#[path = "motion/cases.rs"]
mod cases;
#[path = "motion/sequences.rs"]
mod sequences;

use div::prelude::*;
use sample_motion::motion::{animate, animate_sequence, animate_staggered, Started};
use std::path::PathBuf;

fn out_dir() -> PathBuf {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("out/motion/div");
    std::fs::create_dir_all(&out).unwrap();
    out
}

/// A 200x120 document as the Chromium page sets it up: the root has
/// `html, body { margin: 0; overflow: hidden }` and a white background.
fn stage() -> Document {
    let document = Document::with_viewport_metrics(
        ViewportMetrics::from_logical_size(200.0, 120.0, 1.0).unwrap(),
    )
    .unwrap();
    // No viewport scrollbar when a transform leaves the viewport.
    document
        .body()
        .apply_style(&style! { overflow: hidden })
        .unwrap();
    document.body().set_background_color(Color::WHITE).unwrap();
    document
}

/// The `.b` boxes of tools/motion_oracle.py (`box_position`).
fn boxes(document: &Document, count: usize) -> Vec<Element> {
    (0..count)
        .map(|index| {
            let node = Element::create(document, "div").unwrap();
            let (left, top) = (10 + (index % 6) * 30, 10 + (index / 6) * 50);
            node.apply_style(&style! {
                position: absolute; width: 20px; height: 20px
            })
            .unwrap();
            node.set_property(StyleProperty::Left, LengthValue::px(left as f32).into())
                .unwrap();
            node.set_property(StyleProperty::Top, LengthValue::px(top as f32).into())
                .unwrap();
            node.set_background_color(Color::BLUE).unwrap();
            document.body().append_child(&node).unwrap();
            node
        })
        .collect()
}

#[test]
#[ignore = "writes Div's side of the Chromium comparison"]
fn write_div_frames() {
    let spec = cases::spec();
    let times: Vec<f64> = spec["pixels"]["times"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_f64().unwrap())
        .collect();
    let names: Vec<&str> = spec["pixels"]["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap())
        .collect();
    let all = cases::cases();
    let out = out_dir();
    for name in names {
        let case = all.iter().find(|case| case.name == name).unwrap();
        for &time in &times {
            let document = stage();
            let node = Element::create(&document, "div").unwrap();
            node.apply_style(&style! {
                position: absolute; left: 20px; top: 20px; width: 60px; height: 60px
            })
            .unwrap();
            node.set_background_color(Color::BLUE).unwrap();
            document.body().append_child(&node).unwrap();
            let started = animate(&node, &case.value, &case.transition).unwrap();
            assert!(
                matches!(started, Started::Animation(_)),
                "{name}: {started:?}"
            );
            document.advance_time(time).unwrap();
            std::fs::write(
                out.join(format!("{name}-{time}.png")),
                document.render_to_png_buffer().unwrap(),
            )
            .unwrap();
        }
    }
}

#[test]
#[ignore = "writes Div's side of the Chromium comparison"]
fn write_div_sequence_frames() {
    let spec = sequences::spec();
    let times: Vec<f64> = spec["pixels"]["times"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_f64().unwrap())
        .collect();
    let staggers = sequences::stagger_cases();
    let sequence_cases = sequences::sequence_cases();
    let out = out_dir();
    for name in spec["pixels"]["cases"].as_array().unwrap() {
        let name = name.as_str().unwrap();
        for &time in &times {
            let document = stage();
            if let Some(case) = staggers.iter().find(|case| case.name == name) {
                let elements = boxes(&document, case.elements);
                animate_staggered(&elements, &case.value, &case.transition, &case.stagger).unwrap();
            } else {
                let case = sequence_cases
                    .iter()
                    .find(|case| case.name == name)
                    .unwrap();
                let elements = boxes(&document, case.elements);
                animate_sequence(&elements, &case.sequence, &case.options).unwrap();
            }
            document.advance_time(time).unwrap();
            std::fs::write(
                out.join(format!("{name}-{time}.png")),
                document.render_to_png_buffer().unwrap(),
            )
            .unwrap();
        }
    }
}
