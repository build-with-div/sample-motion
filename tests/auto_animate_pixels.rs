//! Div's side of the AutoAnimate frame comparison: the port runs the
//! `pixels` cases of tests/auto_animate/cases.json on Div's headless clock
//! and saves `out/auto_animate/div/<case>-<frame>.png` at the listed frames.
//! `tools/auto_animate_oracle.py pixels` captures the same frames from
//! AutoAnimate in Chromium and compares.
//!
//! ```text
//! cargo test --test auto_animate_pixels -- --ignored
//! ```

#[allow(dead_code)]
#[path = "auto_animate/cases.rs"]
mod cases;

use cases::Run;
use std::path::PathBuf;

#[test]
#[ignore = "writes Div's side of the Chromium comparison"]
fn write_div_frames() {
    let spec = cases::spec();
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("out/auto_animate/div");
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
        let (mut run, _) = Run::new(&spec, case);
        for frame in 0..=last {
            if frame > 0 {
                run.frame(frame);
            }
            if frames.contains(&frame) {
                std::fs::write(
                    out.join(format!("{name}-{frame}.png")),
                    run.document.render_to_png_buffer().unwrap(),
                )
                .unwrap();
            }
        }
    }
}
