//! A port of react-spring 10.1.2's core: `SpringValue` for numbers, the
//! frame loop (`@react-spring/shared` `frameLoop`) and the frame scheduler
//! (`@react-spring/rafz`).
//!
//! The port keeps react-spring's formulas and its order of operations, so a
//! spring's value at each frame equals react-spring's in Chromium given the
//! same frame times (`tests/react_spring_values.rs`). The React layer
//! (`useSpring`, `animated`, `Controller`) is not ported; [`dom`] writes a
//! spring's value to an element's inline style, which is what `animated`
//! does on each change.
//!
//! JavaScript keeps rafz and the frame loop in module state. Here they live
//! in one [`Runtime`]. The host drives it the way a browser drives
//! JavaScript:
//!
//! - [`Runtime::animation_frame`] runs the animation frame callbacks that
//!   rafz queued with `requestAnimationFrame`, at the frame's time;
//! - [`Runtime::set_time`] is `performance.now()` for work outside a frame;
//! - [`Runtime::perform_microtask_checkpoint`] runs the promise reactions
//!   that continue a chain, after the host's own calls.

pub mod config;
pub mod dom;
pub mod frame;
mod rafz;
mod spring;

pub use config::{preset, AnimationConfig, ConfigProps, Decay, Easing};
pub use spring::{SpringId, SpringProps, To, Unsupported};

use rafz::{Raf, Task};
use spring::{ChainStep, SpringValue};
use std::collections::VecDeque;

/// `frameLoop` state. Every spring has priority 0 here, so the start queue
/// and priority sorting never come into play.
#[derive(Debug, Default)]
struct FrameLoop {
    current: Vec<SpringId>,
    prev: Vec<SpringId>,
}

/// react-spring's global state: rafz, the frame loop, and the springs.
#[derive(Debug, Default)]
pub struct Runtime {
    raf: Raf,
    frame_loop: FrameLoop,
    springs: Vec<SpringValue>,
    microtasks: VecDeque<ChainStep>,
    /// `performance.now()`.
    now: f64,
}

impl Runtime {
    pub fn new() -> Self {
        Self::default()
    }

    /// `performance.now()` for the calls that follow, outside a frame.
    pub fn set_time(&mut self, now: f64) {
        self.now = now;
    }

    /// `new SpringValue(props)`.
    pub fn create(&mut self, props: SpringProps) -> Result<SpringId, Unsupported> {
        props.check()?;
        Ok(self.create_spring(props))
    }

    /// `new SpringValue(from)`: `{ from, default: true }`.
    pub fn create_at(&mut self, from: f64) -> SpringId {
        self.create_spring(SpringProps {
            from: Some(from),
            ..SpringProps::default()
        })
    }

    /// `spring.start(props)`.
    pub fn start(&mut self, id: SpringId, props: SpringProps) -> Result<(), Unsupported> {
        props.check()?;
        self.update_spring(id, props, spring::Resolver::None);
        Ok(())
    }

    /// `spring.stop()`.
    pub fn stop(&mut self, id: SpringId) {
        self.stop_spring(id);
    }

    /// `spring.get()`.
    pub fn get(&self, id: SpringId) -> Option<f64> {
        self.springs[id.0].get()
    }

    /// `spring.velocity`, in units per millisecond.
    pub fn velocity(&self, id: SpringId) -> f64 {
        self.springs[id.0].velocity()
    }

    /// `spring.isAnimating`.
    pub fn is_animating(&self, id: SpringId) -> bool {
        self.springs[id.0].is_animating()
    }

    /// `spring.idle`.
    pub fn idle(&self, id: SpringId) -> bool {
        self.springs[id.0].idle()
    }

    /// Whether rafz has a frame callback queued, that is whether the host
    /// needs to keep producing animation frames.
    pub fn wants_frame(&self) -> bool {
        self.raf.native_callbacks > 0
    }

    /// div-gap: R1 requestAnimationFrame. Run the animation frame callbacks
    /// rafz queued with `requestAnimationFrame`, at frame time `now`
    /// (milliseconds), with a microtask checkpoint after each, as HTML's
    /// "run the animation frame callbacks" does. Callbacks queued while
    /// they run wait for the next frame. Div has no frame callback yet: the
    /// host calls this from its frame source (the headless clock in tests,
    /// `PlatformEvent::Presented` on the desktop).
    pub fn animation_frame(&mut self, now: f64) {
        self.now = now;
        let callbacks = std::mem::take(&mut self.raf.native_callbacks);
        for _ in 0..callbacks {
            self.raf_loop();
            self.perform_microtask_checkpoint();
        }
    }

    /// Run the queued promise reactions: the next step of each chain whose
    /// current step finished.
    pub fn perform_microtask_checkpoint(&mut self) {
        while let Some(step) = self.microtasks.pop_front() {
            self.chain_step(step);
        }
    }

    /// rafz `loop()`.
    fn raf_loop(&mut self) {
        // `~ts`
        if self.raf.ts != -1.0 {
            // `nativeRaf(loop)`
            self.raf.native_callbacks += 1;
            self.raf_update();
        }
    }

    /// rafz `update()`.
    fn raf_update(&mut self) {
        let prev_ts = self.raf.ts;
        self.raf.ts = self.now;
        let count = self.raf.find_timeout(self.raf.ts);
        if count > 0 {
            let due: Vec<_> = self.raf.timeouts.drain(..count).collect();
            for timeout in due {
                self.run_scheduled(timeout.handler, Some(timeout.id));
            }
            self.raf.pending_count -= count as i64;
        }
        if self.raf.pending_count == 0 {
            // `stop()`
            self.raf.ts = -1.0;
            return;
        }
        // `prevTs ? Math.min(64, ts - prevTs) : 16.667`
        let dt = if prev_ts != 0.0 {
            (self.raf.ts - prev_ts).min(64.0)
        } else {
            16.667
        };
        self.flush_update_queue(dt);
    }

    /// `updateQueue.flush(dt)`.
    fn flush_update_queue(&mut self, dt: f64) {
        if self.raf.update_queue.next.is_empty() {
            return;
        }
        let current = std::mem::take(&mut self.raf.update_queue.next);
        self.raf.update_queue.flushing = true;
        self.raf.pending_count -= current.len() as i64;
        for task in current {
            let keep = match task {
                Task::Advance => self.frame_loop_advance(dt),
            };
            if keep && !self.raf.update_queue.next.contains(&task) {
                self.raf.update_queue.next.push(task);
            }
        }
        self.raf.pending_count += self.raf.update_queue.next.len() as i64;
        self.raf.update_queue.flushing = false;
    }

    /// `frameLoop.start(animation)`.
    fn frame_loop_start(&mut self, id: SpringId) {
        // `startSafely`: priorities are all 0, so it goes last.
        if !self.frame_loop.current.contains(&id) {
            self.frame_loop.current.push(id);
        }
        // `raf(advance)`
        self.schedule_advance();
    }

    /// `frameLoop.advance(dt)`: returns true while any spring is active.
    fn frame_loop_advance(&mut self, dt: f64) -> bool {
        let mut next = std::mem::take(&mut self.frame_loop.prev);
        let mut i = 0;
        while i < self.frame_loop.current.len() {
            let id = self.frame_loop.current[i];
            if !self.spring_idle(id) {
                self.advance_spring(id, dt);
                if !self.spring_idle(id) {
                    next.push(id);
                }
            }
            i += 1;
        }
        let mut prev = std::mem::replace(&mut self.frame_loop.current, next);
        prev.clear();
        self.frame_loop.prev = prev;
        !self.frame_loop.current.is_empty()
    }
}
