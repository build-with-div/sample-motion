//! Where anime.js's animation frames come from on Div.
//!
//! anime.js's engine runs from `requestAnimationFrame` and reads time with
//! `Date.now()`. Div has no frame callback yet (plan R1), so this scaffold
//! stands in for it and is deleted when R1 lands. See
//! docs/framework-gaps.md.

use super::Runtime;
use div::prelude::*;

/// div-gap: R1 requestAnimationFrame. A headless frame: move the document's
/// animation clock to `time_ms`, then run the animation frame callbacks
/// with `Date.now()` at the clock's time in whole milliseconds.
pub fn headless_frame(
    document: &Document,
    runtime: &mut Runtime,
    time_ms: f64,
) -> Result<(), String> {
    document
        .advance_time(time_ms)
        .map_err(|error| error.to_string())?;
    let now = document.get_time().map_err(|error| error.to_string())?;
    runtime.animation_frame(now.floor());
    Ok(())
}
