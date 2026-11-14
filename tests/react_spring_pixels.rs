//! Div's side of the react-spring frame comparison: the port runs the
//! `pixels` cases of tests/react_spring/cases.json on Div's headless clock,
//! writes each spring's value to the box's inline style after every frame,
//! and saves `out/react-spring/div/<case>-<frame>.png` at the listed frames.
//! `tools/react_spring_oracle.py pixels` captures the same frames from
//! react-spring in Chromium and compares.
//!
//! ```text
//! cargo test --test react_spring_pixels -- --ignored
//! ```

#[allow(dead_code)]
#[path = "react_spring/cases.rs"]
mod cases;

use cases::Run;
use div::prelude::*;
use sample_motion::react_spring::dom::StyleTarget;
use sample_motion::react_spring::frame::headless_frame;
use std::path::PathBuf;

/// The page of tools/react_spring_oracle.py: a 200x120 white viewport and a
/// 60 px blue box at (20, 20).
fn stage() -> (Document, Element) {
    let document = Document::with_viewport_metrics(
        ViewportMetrics::from_logical_size(200.0, 120.0, 1.0).unwrap(),
    )
    .unwrap();
    document
        .body()
        .apply_style(&style! { overflow: hidden })
        .unwrap();
    document.body().set_background_color(Color::WHITE).unwrap();
    let node = Element::create(&document, "div").unwrap();
    node.apply_style(&style! {
        position: absolute; left: 20px; top: 20px; width: 60px; height: 60px
    })
    .unwrap();
    node.set_background_color(Color::BLUE).unwrap();
    document.body().append_child(&node).unwrap();
    (document, node)
}

fn write(run: &Run, node: &Element) {
    for (spring, target) in run.springs.iter().zip(&run.targets) {
        let value = run.runtime.get(*spring).unwrap();
        StyleTarget::from_name(target)
            .unwrap()
            .write(node, value)
            .unwrap();
    }
}

#[test]
#[ignore = "writes Div's side of the Chromium comparison"]
fn write_div_frames() {
    let spec = cases::spec();
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("out/react-spring/div");
    std::fs::create_dir_all(&out).unwrap();
    let frames: Vec<usize> = spec["pixels"]["frames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_u64().unwrap() as usize)
        .collect();
    let last = *frames.iter().max().unwrap();
    let all = spec["cases"].as_array().unwrap();
    for name in spec["pixels"]["cases"].as_array().unwrap() {
        let name = name.as_str().unwrap();
        let case = all.iter().find(|case| case["name"] == name).unwrap();
        let (document, node) = stage();
        let mut run = Run::new(case);
        for frame in 0..=last {
            if frame > 0 {
                let time = run.time(frame);
                headless_frame(&document, &mut run.runtime, time).unwrap();
                assert_eq!(
                    document.get_time().unwrap(),
                    time,
                    "{name}: the headless clock"
                );
                run.actions_after_frame(frame);
            }
            write(&run, &node);
            if frames.contains(&frame) {
                std::fs::write(
                    out.join(format!("{name}-{frame}.png")),
                    document.render_to_png_buffer().unwrap(),
                )
                .unwrap();
            }
        }
    }
}
