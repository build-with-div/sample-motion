//! Where react-spring's animation frames come from on Div.
//!
//! react-spring runs from `requestAnimationFrame`. Div has no frame
//! callback yet (plan R1), so these scaffolds stand in for it and are
//! deleted when R1 lands. See docs/framework-gaps.md.

use super::Runtime;
use div::prelude::*;
use div::typed_style::{AnimationOptions, Easing, FillMode, IterationCount, Keyframes};
use div::PlatformEvent;

/// div-gap: R1 requestAnimationFrame. A headless frame: move the document's
/// animation clock to `time_ms`, then run the animation frame callbacks at
/// the clock's time.
pub fn headless_frame(
    document: &Document,
    runtime: &mut Runtime,
    time_ms: f64,
) -> Result<(), String> {
    document
        .advance_time(time_ms)
        .map_err(|error| error.to_string())?;
    let now = document.get_time().map_err(|error| error.to_string())?;
    runtime.animation_frame(now);
    Ok(())
}

/// div-gap: R1 requestAnimationFrame. The frame time of a desktop frame:
/// `PlatformEvent::Presented`, which arrives after the frame is presented,
/// so a value written from it shows one frame late.
pub fn presented_time(event: &PlatformEvent) -> Option<f64> {
    match event {
        PlatformEvent::Presented { time_ms, .. } => Some(*time_ms),
        _ => None,
    }
}

/// div-gap: R1 requestAnimationFrame. The desktop presents frames only
/// while something changes, so an engine animation that never ends keeps
/// `Presented` coming: a 1 px element under `parent` whose opacity
/// animates forever.
pub fn keep_alive(document: &Document, parent: &Element) -> Result<Element, String> {
    let element = Element::create(document, "div").map_err(|error| error.to_string())?;
    element
        .apply_style(&style! { position: absolute; width: 1px; height: 1px })
        .map_err(|error| error.to_string())?;
    parent
        .append_child(&element)
        .map_err(|error| error.to_string())?;
    element
        .animate(
            StyleProperty::Opacity,
            Keyframes::from_values(0.0_f32, 0.01_f32),
            AnimationOptions {
                duration_ms: 1000.0,
                iterations: IterationCount::Infinite,
                fill: FillMode::Both,
                easing: Easing::Linear,
                ..AnimationOptions::default()
            },
        )
        .map_err(|error| error.to_string())?;
    Ok(element)
}
