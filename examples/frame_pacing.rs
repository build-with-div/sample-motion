//! Measurement: native frame pacing and timer accuracy.
//!
//! Opens a window, animates for a few seconds, prints statistics and exits.
//!
//! ```text
//! cargo run --release --example frame_pacing -- [mode] [count] [seconds] [backend] [block_ms]
//!   mode     engine  | N cells with infinite engine transform animations (default)
//!            signal  | N cells moved from the Presented callback (imperative tween),
//!                      kept alive by one engine animation
//!            timers  | no animation: measure task::time::interval(16 ms) and sleep(1 ms)
//!   count    number of animated cells (default 100)
//!   seconds  measuring time (default 5)
//!   backend  auto | gl | software (default auto)
//!   block_ms busy-wait this long on the UI thread every 30th frame (default 0)
//! ```
//!
//! Frame times come from `PlatformEvent::Presented { time_ms }` (the clock the
//! desktop loop passes to `Document::begin_frame`) and from `Instant` taken in
//! the handler. Div has no vsync timestamp, so neither is a present time.

use div::prelude::*;
use div::task::{spawn_local, time};
use div::typed_style::{
    AnimationOptions, Easing, FillMode, IterationCount, Keyframes, PlaybackDirection,
    TransformList, TransformOperation,
};
use div::{BackendPreference, PlatformEvent};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

fn shift(x: f32) -> TransformList {
    TransformList(vec![TransformOperation::Translate(
        LengthValue::px(x),
        LengthValue::px(0.0),
    )])
}

fn infinite() -> AnimationOptions {
    AnimationOptions {
        duration_ms: 1000.0,
        iterations: IterationCount::Infinite,
        direction: PlaybackDirection::Alternate,
        fill: FillMode::Both,
        easing: Easing::Linear,
        ..AnimationOptions::default()
    }
}

fn percentile(sorted: &[f64], p: usize) -> f64 {
    sorted[(sorted.len() * p / 100).min(sorted.len() - 1)]
}

fn report(label: &str, mut deltas: Vec<f64>, budget_ms: f64) {
    if deltas.is_empty() {
        println!("{label}: no samples");
        return;
    }
    let total: f64 = deltas.iter().sum();
    deltas.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let late = deltas.iter().filter(|d| **d > budget_ms * 1.5).count();
    println!(
        "{label}: n={} mean {:.2} ms ({:.1}/s), median {:.2}, p95 {:.2}, p99 {:.2}, max {:.2}, \
         min {:.2}, >{:.1} ms: {late}",
        deltas.len(),
        total / deltas.len() as f64,
        deltas.len() as f64 * 1000.0 / total,
        percentile(&deltas, 50),
        percentile(&deltas, 95),
        percentile(&deltas, 99),
        deltas[deltas.len() - 1],
        deltas[0],
        budget_ms * 1.5,
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let process_start = Instant::now();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map_or("engine", String::as_str).to_owned();
    let count: usize = args.get(1).map_or(Ok(100), |v| v.parse())?;
    let seconds: f64 = args.get(2).map_or(Ok(5.0), |v| v.parse())?;
    let backend = match args.get(3).map_or("auto", String::as_str) {
        "gl" => BackendPreference::OpenGl,
        "software" => BackendPreference::Software,
        _ => BackendPreference::Auto,
    };
    let block_ms: f64 = args.get(4).map_or(Ok(0.0), |v| v.parse())?;
    println!(
        "mode {mode}, count {count}, {seconds} s, backend {:?}, block {block_ms} ms/30 frames",
        backend
    );

    let mut app = App::builder()
        .title("frame pacing")
        .size(LogicalSize::new(1280.0, 800.0))
        .backend(backend)
        .build()?;
    let exit = app.exit_handle();
    let cells: Rc<RefCell<Vec<Element>>> = Rc::default();
    let frames: Rc<RefCell<Vec<(f64, Instant)>>> = Rc::default();
    let timer_ticks: Rc<RefCell<Vec<Instant>>> = Rc::default();
    let sleeps: Rc<RefCell<Vec<f64>>> = Rc::default();

    {
        let cells = cells.clone();
        let frames = frames.clone();
        let mode = mode.clone();
        let exit = exit.clone();
        let started = Instant::now();
        app.on_platform_event(move |event| {
            let PlatformEvent::Presented { time_ms, .. } = event else {
                return;
            };
            let now = Instant::now();
            let mut frames = frames.borrow_mut();
            frames.push((*time_ms, now));
            if mode == "signal" {
                // Imperative tween: what a spring or gesture driver would do.
                let phase = (time_ms / 1000.0) % 2.0;
                let x = if phase < 1.0 { phase } else { 2.0 - phase } as f32 * 8.0;
                for cell in cells.borrow().iter() {
                    cell.set_property(StyleProperty::Transform, shift(x).into())
                        .unwrap();
                }
            }
            if block_ms > 0.0 && frames.len().is_multiple_of(30) {
                let until = Instant::now() + Duration::from_secs_f64(block_ms / 1000.0);
                while Instant::now() < until {
                    std::hint::spin_loop();
                }
            }
            if started.elapsed().as_secs_f64() > seconds + 0.5 {
                exit.request_exit();
            }
        });
    }

    let run_mode = mode.clone();
    let view_cells = cells.clone();
    let ticks = timer_ticks.clone();
    let sleep_samples = sleeps.clone();
    let timer_exit = exit.clone();
    app.run(move || {
        let document = current_document();
        let grid = Element::create(&document, "div").unwrap();
        grid.apply_style(&style! { display: flex; flex-wrap: wrap; width: 1280px })
            .unwrap();
        let keep_alive = Element::create(&document, "div").unwrap();
        keep_alive
            .apply_style(&style! { width: 1px; height: 1px })
            .unwrap();
        grid.append_child(&keep_alive).unwrap();
        if run_mode != "timers" {
            keep_alive
                .animate(
                    StyleProperty::Opacity,
                    Keyframes::from_values(0.0_f32, 1.0_f32),
                    infinite(),
                )
                .unwrap();
            for index in 0..count {
                let cell = Element::create(&document, "div").unwrap();
                cell.apply_style(&style! { width: 24px; height: 24px; margin: 2px })
                    .unwrap();
                cell.set_background_color(Color::BLUE).unwrap();
                grid.append_child(&cell).unwrap();
                if run_mode == "engine" {
                    let mut options = infinite();
                    options.delay_ms = -(((index * 37) % 1000) as f64);
                    cell.animate(
                        StyleProperty::Transform,
                        Keyframes::from_values(shift(0.0), shift(8.0)),
                        options,
                    )
                    .unwrap();
                }
                view_cells.borrow_mut().push(cell);
            }
        } else {
            spawn_local(async move {
                let start = Instant::now();
                let mut interval = time::interval(Duration::from_millis(16));
                while start.elapsed().as_secs_f64() < seconds / 2.0 {
                    interval.tick().await;
                    ticks.borrow_mut().push(Instant::now());
                }
                while start.elapsed().as_secs_f64() < seconds {
                    let before = Instant::now();
                    time::sleep(Duration::from_millis(1)).await;
                    sleep_samples
                        .borrow_mut()
                        .push(before.elapsed().as_secs_f64() * 1000.0);
                }
                timer_exit.request_exit();
            })
            .detach();
        }
        ViewNode::Element(grid)
    })?;

    let frames = frames.borrow();
    if let (Some(first), Some(last)) = (frames.first(), frames.last()) {
        println!(
            "frames presented: {}, first at {:.0} ms after start, last at {:.0} ms",
            frames.len(),
            first.1.duration_since(process_start).as_secs_f64() * 1000.0,
            last.1.duration_since(process_start).as_secs_f64() * 1000.0,
        );
    }
    // Skip the first half second (window creation, first raster).
    let skip = frames
        .iter()
        .position(|(_, at)| at.duration_since(frames[0].1).as_secs_f64() > 0.5)
        .unwrap_or(0);
    let measured = &frames[skip..];
    let clock: Vec<f64> = measured.windows(2).map(|w| w[1].0 - w[0].0).collect();
    let wall: Vec<f64> = measured
        .windows(2)
        .map(|w| w[1].1.duration_since(w[0].1).as_secs_f64() * 1000.0)
        .collect();
    if mode != "timers" {
        report("frame interval (frame clock)", clock, 1000.0 / 60.0);
        report("frame interval (handler wall)", wall, 1000.0 / 60.0);
    } else {
        println!("frames presented while idle: {}", frames.len());
        let ticks = timer_ticks.borrow();
        let deltas: Vec<f64> = ticks
            .windows(2)
            .map(|w| w[1].duration_since(w[0]).as_secs_f64() * 1000.0)
            .collect();
        report("interval(16 ms) tick spacing", deltas, 16.0);
        report("sleep(1 ms) actual", sleeps.borrow().clone(), 1.0);
    }
    Ok(())
}
