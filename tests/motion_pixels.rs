//! Div's side of the Motion pixel comparison: the port animates the same
//! box as `tools/motion_oracle.py pixels` does with Motion in Chromium, and
//! writes `out/motion/div/<case>-<t>.png` on Div's headless clock.
//!
//! ```text
//! cargo test --test motion_pixels -- --ignored
//! ```

#[path = "motion/cases.rs"]
mod cases;

use div::prelude::*;
use sample_motion::motion::{animate, Started};
use std::path::PathBuf;

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
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("out/motion/div");
    std::fs::create_dir_all(&out).unwrap();
    for name in names {
        let case = all.iter().find(|case| case.name == name).unwrap();
        for &time in &times {
            let document = Document::with_viewport_metrics(
                ViewportMetrics::from_logical_size(200.0, 120.0, 1.0).unwrap(),
            )
            .unwrap();
            // The Chromium page has `html, body { margin: 0; overflow: hidden }`:
            // no viewport scrollbar when an overshooting transform leaves the
            // viewport.
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
