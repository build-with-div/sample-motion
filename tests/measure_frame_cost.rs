//! Measurement: per-frame cost of N concurrently animating elements, headless.
//!
//! Ignored by default; run it deliberately and record the conditions:
//!
//! ```text
//! cargo test --release --test measure_frame_cost -- --ignored --nocapture --test-threads 1
//! ```
//!
//! Each frame is split along Div's public API, which is as far as an app can
//! see into the pipeline:
//!
//! - `sample`: `Document::advance_time` — sample effects and re-cascade the
//!   authored styles (`Engine::set_animation_time`).
//! - `update`: `Document::update_all` — layout when dirty, then re-record the
//!   whole paint (`Engine::update`). Layout and paint are not separable here;
//!   compare a transform run (no layout) with a width run (layout).
//! - `raster`: `Document::render_to_bitmap` — build the scene and raster the
//!   full viewport on the CPU (a fresh `SoftwareCompositor` per call).
//!
//! Present is not included: headless has none. `examples/frame_pacing.rs`
//! measures the native loop.

use div::prelude::*;
use div::typed_style::{
    AnimationOptions, Easing, FillMode, IterationCount, Keyframes, PlaybackDirection,
    TransformList, TransformOperation,
};
use std::time::{Duration, Instant};

const WIDTH: f64 = 1280.0;
const HEIGHT: f64 = 800.0;
const FRAMES: usize = 120;
const WARMUP: usize = 10;

#[derive(Clone, Copy, Debug)]
enum Kind {
    Transform,
    Opacity,
    BackgroundColor,
    Width,
}

fn shift(x: f32) -> TransformList {
    TransformList(vec![TransformOperation::Translate(
        LengthValue::px(x),
        LengthValue::px(0.0),
    )])
}

fn scene(count: usize, kind: Kind) -> Document {
    scene_with(count, kind, true)
}

fn scene_with(count: usize, kind: Kind, animate: bool) -> Document {
    let document = Document::with_font_collection(
        ViewportMetrics::from_logical_size(WIDTH, HEIGHT, 1.0).unwrap(),
        FontCollection::deterministic_test(),
    )
    .unwrap();
    document.body().set_background_color(Color::WHITE).unwrap();
    let grid = Element::create(&document, "div").unwrap();
    grid.apply_style(&style! { display: flex; flex-wrap: wrap; width: 1280px })
        .unwrap();
    document.body().append_child(&grid).unwrap();
    let options = AnimationOptions {
        duration_ms: 1000.0,
        iterations: IterationCount::Infinite,
        direction: PlaybackDirection::Alternate,
        fill: FillMode::Both,
        easing: Easing::Linear,
        ..AnimationOptions::default()
    };
    for index in 0..count {
        let cell = Element::create(&document, "div").unwrap();
        cell.apply_style(&style! { width: 24px; height: 24px; margin: 2px })
            .unwrap();
        cell.set_background_color(Color::BLUE).unwrap();
        grid.append_child(&cell).unwrap();
        if !animate {
            continue;
        }
        let mut options = options.clone();
        // Spread phases so values differ per element.
        options.delay_ms = -(((index * 37) % 1000) as f64);
        match kind {
            Kind::Transform => cell.animate(
                StyleProperty::Transform,
                Keyframes::from_values(shift(0.0), shift(8.0)),
                options,
            ),
            Kind::Opacity => cell.animate(
                StyleProperty::Opacity,
                Keyframes::from_values(0.2_f32, 1.0_f32),
                options,
            ),
            Kind::BackgroundColor => cell.animate(
                StyleProperty::BackgroundColor,
                Keyframes::from_values(Color::BLUE, Color::from_rgba8(255, 0, 0, 255)),
                options,
            ),
            Kind::Width => cell.animate(
                StyleProperty::Width,
                Keyframes::from_values(LengthValue::px(16.0), LengthValue::px(24.0)),
                options,
            ),
        }
        .unwrap();
    }
    document.update_all().unwrap();
    document
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn stats(mut values: Vec<f64>) -> (f64, f64) {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = values[values.len() / 2];
    let p95 = values[(values.len() * 95 / 100).min(values.len() - 1)];
    (median, p95)
}

fn run(count: usize, kind: Kind) -> [(f64, f64); 4] {
    let document = scene(count, kind);
    let (mut sample, mut update, mut raster, mut total) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for frame in 0..WARMUP + FRAMES {
        let time = frame as f64 * 1000.0 / 60.0;
        let start = Instant::now();
        document.advance_time(time).unwrap();
        let sampled = Instant::now();
        document.update_all().unwrap();
        let updated = Instant::now();
        let bitmap = document.render_to_bitmap().unwrap();
        let rastered = Instant::now();
        std::hint::black_box(bitmap);
        if frame >= WARMUP {
            sample.push(millis(sampled - start));
            update.push(millis(updated - sampled));
            raster.push(millis(rastered - updated));
            total.push(millis(rastered - start));
        }
    }
    [stats(sample), stats(update), stats(raster), stats(total)]
}

#[test]
#[ignore = "measurement; run with --ignored --nocapture"]
fn frame_cost_by_element_count_and_property() {
    println!(
        "viewport {WIDTH}x{HEIGHT} @1x, {FRAMES} frames after {WARMUP} warm-up, 60 Hz steps; \
         profile: {}",
        if cfg!(debug_assertions) {
            "debug (deps opt-level 2)"
        } else {
            "release"
        }
    );
    println!("| property | N | sample ms (med/p95) | update ms | raster ms | total ms |");
    println!("| --- | ---: | ---: | ---: | ---: | ---: |");
    for kind in [
        Kind::Transform,
        Kind::Opacity,
        Kind::BackgroundColor,
        Kind::Width,
    ] {
        for count in [1, 10, 100, 1000] {
            let [s, u, r, t] = run(count, kind);
            println!(
                "| {kind:?} | {count} | {:.2} / {:.2} | {:.2} / {:.2} | {:.2} / {:.2} | {:.2} / {:.2} |",
                s.0, s.1, u.0, u.1, r.0, r.1, t.0, t.1
            );
        }
    }
}

#[test]
#[ignore = "measurement; run with --ignored --nocapture"]
fn idle_frame_cost_with_no_animation() {
    // The floor: one static page re-rendered, as the native loop does when
    // anything at all changes (no damage tracking).
    let document = scene(0, Kind::Transform);
    let mut raster = Vec::new();
    for frame in 0..WARMUP + FRAMES {
        let start = Instant::now();
        std::hint::black_box(document.render_to_bitmap().unwrap());
        if frame >= WARMUP {
            raster.push(millis(start.elapsed()));
        }
    }
    let (median, p95) = stats(raster);
    println!("static {WIDTH}x{HEIGHT} raster: median {median:.2} ms, p95 {p95:.2} ms");
}

#[test]
#[ignore = "measurement; run with --ignored --nocapture"]
fn start_cost_by_animation_count() {
    // Time to build N cells and start one infinite animation on each, then
    // the first update. The native first frame waits for this.
    println!("| N | cells only ms | cells + animate ms | per animate µs |");
    println!("| ---: | ---: | ---: | ---: |");
    for count in [100, 300, 1000] {
        let start = Instant::now();
        let document = scene_with(count, Kind::Transform, false);
        drop(document);
        let cells = millis(start.elapsed());
        let start = Instant::now();
        let document = scene_with(count, Kind::Transform, true);
        drop(document);
        let animated = millis(start.elapsed());
        println!(
            "| {count} | {cells:.1} | {animated:.1} | {:.0} |",
            (animated - cells) * 1000.0 / count as f64
        );
    }
}
