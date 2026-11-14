//! A port of anime.js 4.5.0's JavaScript engine: `animate()`,
//! `createTimeline()`, `stagger()` and the eases, for DOM targets'
//! `opacity` and transform properties.
//!
//! The port keeps anime.js's formulas, rounding and order of operations, so
//! the style strings it writes at each frame equal the strings anime.js
//! writes in Chromium given the same `Date.now()` and frames
//! (`tests/anime_values.rs`). anime.js keeps its engine in module state;
//! here it lives in one [`Runtime`], which the host drives as a browser
//! drives JavaScript:
//!
//! - [`Runtime::animation_frame`] runs the frame callback the engine queued
//!   with `requestAnimationFrame`, with `Date.now()` at the frame;
//! - [`Runtime::set_time`] is `Date.now()` for calls outside a frame;
//! - the [`StyleHost`] receives the inline style writes (`el.style.opacity
//!   = ...`, `el.style.transform = ...`) and answers computed style reads.
//!
//! Not ported: colours, complex values, CSS variables, attributes, plain
//! objects, unit conversion that needs layout, blend composition, spring
//! and irregular eases, callbacks and promises, `seek`/`reverse`/
//! `alternate`/`stretch`, timers without targets, and the WAAPI
//! `waapi.animate()`. They return [`Unsupported`].

mod animation;
pub mod dom;
pub mod ease;
mod engine;
pub mod frame;
pub mod stagger;
mod timeline;
pub mod value;

pub use engine::{Runtime, TimerId};
pub use value::{Obj, Raw, Unsupported, Value};

/// A target element, by index in the host's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TargetId(pub usize);

/// One inline style write: `el.style[property] = value`.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleWrite {
    pub target: TargetId,
    pub property: String,
    pub value: String,
}

/// The page anime.js animates.
pub trait StyleHost {
    /// `getComputedStyle(target).getPropertyValue(property)`, read when an
    /// animation starts from a property's current value and the element
    /// has no inline value for it.
    fn computed_value(&self, target: TargetId, property: &str) -> String;
    /// `target.style[property] = value`.
    fn set_style(&mut self, target: TargetId, property: &str, value: &str);
    /// The writes the host could not apply.
    fn errors(&self) -> &[String] {
        &[]
    }
}

impl Runtime {
    /// `animate(targets, parameters)`.
    pub fn animate(
        &mut self,
        targets: &[TargetId],
        parameters: &Obj,
    ) -> Result<TimerId, Unsupported> {
        let id = self.new_animation(targets, parameters, None, 0.0, 0, None)?;
        self.init(id, false);
        Ok(id)
    }

    /// The host's errors: writes it could not apply.
    pub fn host_errors(&self) -> &[String] {
        self.host.errors()
    }
}
