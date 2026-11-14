//! `@react-spring/rafz` 10.1.2: the frame scheduler react-spring runs on.
//!
//! rafz keeps module-level state in JavaScript. Here it is one [`Raf`] value
//! inside the [`Runtime`](super::Runtime), which runs the queued work,
//! because that work reaches back into the springs.

use super::spring::ScheduledCall;

/// A function in one of rafz's queues. The port schedules one function,
/// `frameLoop.advance`, on the update queue. rafz's `onStart`, `onFrame`,
/// `write` and `onFinish` queues stay empty for a `SpringValue` with no
/// priority, so they are not modelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Task {
    /// `frameLoop.advance`.
    Advance,
}

/// `makeQueue()`: a set whose `flush` keeps each function that returns true
/// for the next frame. While a flush runs, `current` and `next` are
/// different sets and adding to `next` does not count as pending.
#[derive(Debug, Default)]
pub(crate) struct Queue {
    pub next: Vec<Task>,
    pub flushing: bool,
}

/// A `raf.setTimeout` entry.
#[derive(Debug)]
pub(crate) struct Timeout {
    pub id: u64,
    pub time: f64,
    pub handler: ScheduledCall,
}

/// rafz's module state.
#[derive(Debug)]
pub(crate) struct Raf {
    /// `ts`: the most recent timestamp, `-1` when the loop is stopped.
    pub ts: f64,
    /// `pendingCount`.
    pub pending_count: i64,
    /// `updateQueue`.
    pub update_queue: Queue,
    /// `timeouts`, sorted by time.
    pub timeouts: Vec<Timeout>,
    next_timeout_id: u64,
    /// `nativeRaf(loop)` calls not yet run: rafz's `loop` callbacks queued
    /// with `window.requestAnimationFrame`.
    pub native_callbacks: usize,
}

impl Default for Raf {
    fn default() -> Self {
        Self {
            ts: -1.0,
            pending_count: 0,
            update_queue: Queue::default(),
            timeouts: Vec::new(),
            next_timeout_id: 0,
            native_callbacks: 0,
        }
    }
}

impl Raf {
    /// `start()`.
    pub fn start(&mut self) {
        if self.ts < 0.0 {
            self.ts = 0.0;
            // div-gap: R1 requestAnimationFrame. `nativeRaf(loop)`: the
            // callback waits for the host to run the next animation frame
            // (`Runtime::animation_frame`).
            self.native_callbacks += 1;
        }
    }

    /// `raf(fn)`: `schedule(fn, updateQueue)`.
    pub fn schedule_update(&mut self, task: Task) {
        let queue = &mut self.update_queue;
        if !queue.next.contains(&task) {
            if !queue.flushing {
                self.pending_count += 1;
            }
            queue.next.push(task);
        }
        self.start();
    }

    /// `findTimeout(time)`: the index of the first timeout later than `time`.
    pub fn find_timeout(&self, time: f64) -> usize {
        self.timeouts
            .iter()
            .position(|timeout| timeout.time > time)
            .unwrap_or(self.timeouts.len())
    }

    /// `raf.setTimeout(handler, ms)` at `now` (`raf.now()`).
    pub fn set_timeout(&mut self, now: f64, handler: ScheduledCall, ms: f64) -> u64 {
        let time = now + ms;
        self.next_timeout_id += 1;
        let id = self.next_timeout_id;
        let index = self.find_timeout(time);
        self.timeouts.insert(index, Timeout { id, time, handler });
        self.pending_count += 1;
        self.start();
        id
    }

    /// The timeout's `cancel()`.
    pub fn cancel_timeout(&mut self, id: u64) {
        if let Some(index) = self.timeouts.iter().position(|timeout| timeout.id == id) {
            self.timeouts.remove(index);
            self.pending_count -= 1;
        }
    }
}
