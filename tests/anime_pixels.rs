//! Div's side of the anime.js frame comparison: the port runs the `pixels`
//! cases of tests/anime/cases.json on Div's headless clock, writing its
//! style strings to the boxes through Div's CSS value parser, and saves
//! `out/anime/div/<case>-<frame>.png` at the listed frames.
//! `tools/anime_oracle.py pixels` captures the same frames from anime.js in
//! Chromium and compares.
//!
//! ```text
//! cargo test --test anime_pixels -- --ignored
//! ```

#[allow(dead_code)]
#[path = "anime/cases.rs"]
mod cases;

use cases::Run;
use std::path::PathBuf;

#[test]
#[ignore = "writes Div's side of the Chromium comparison"]
fn write_div_frames() {
    let spec = cases::spec();
    let boxes = spec["boxes"].as_u64().unwrap() as usize;
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("out/anime/div");
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
        let (mut run, _) = Run::new(case, boxes);
        for frame in 0..=last {
            if frame > 0 {
                run.frame(frame);
                assert_eq!(
                    run.document.get_time().unwrap(),
                    cases::time(frame),
                    "{name}: the headless clock"
                );
            }
            if frames.contains(&frame) {
                std::fs::write(
                    out.join(format!("{name}-{frame}.png")),
                    run.document.render_to_png_buffer().unwrap(),
                )
                .unwrap();
            }
        }
        assert!(run.runtime.host_errors().is_empty(), "{name}");
    }
}
