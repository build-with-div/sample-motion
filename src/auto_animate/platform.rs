//! What AutoAnimate takes from the browser and Div does not have yet, as
//! labelled scaffolds, plus small helpers over Div's element API.
//!
//! Each scaffold carries a `div-gap:` comment and an entry in
//! docs/framework-gaps.md. They run on the document clock that the host
//! advances, so the headless tests are deterministic, and they are deleted
//! when Div ships the feature.

use div::prelude::*;
use div::AnimationId;
use std::collections::VecDeque;

/// Element identity, as a JavaScript `Map` or `WeakMap` key compares it.
pub(crate) fn same(a: &Element, b: &Element) -> bool {
    a.is_same_node(b).unwrap_or(false)
}

/// A map keyed by element identity, standing in for AutoAnimate's
/// `WeakMap`s and `WeakSet`s (and its `Set` of parents, which keeps
/// insertion order).
pub(crate) struct NodeMap<V>(Vec<(Element, V)>);

impl<V> NodeMap<V> {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    fn index(&self, key: &Element) -> Option<usize> {
        self.0.iter().position(|(element, _)| same(element, key))
    }

    pub fn get(&self, key: &Element) -> Option<&V> {
        self.index(key).map(|i| &self.0[i].1)
    }

    pub fn has(&self, key: &Element) -> bool {
        self.index(key).is_some()
    }

    pub fn set(&mut self, key: &Element, value: V) {
        match self.index(key) {
            Some(i) => self.0[i].1 = value,
            None => self.0.push((key.clone(), value)),
        }
    }

    pub fn delete(&mut self, key: &Element) -> Option<V> {
        self.index(key).map(|i| self.0.remove(i).1)
    }

    pub fn keys(&self) -> Vec<Element> {
        self.0.iter().map(|(element, _)| element.clone()).collect()
    }
}

/// The element children of `parent`, in order.
pub(crate) fn children(parent: &Element) -> Result<Vec<Element>, div::Error> {
    let mut out = Vec::new();
    let mut child = parent.first_child()?;
    while let Some(element) = child {
        child = element.next_sibling()?;
        out.push(element);
    }
    Ok(out)
}

/// `node.previousSibling` and `node.nextSibling` among its parent's
/// children. The pages have no text nodes between the items.
pub(crate) fn siblings_of(
    node: &Element,
) -> Result<(Option<Element>, Option<Element>), div::Error> {
    let Some(parent) = node.parent()? else {
        return Ok((None, None));
    };
    let list = children(&parent)?;
    let Some(i) = list.iter().position(|child| same(child, node)) else {
        return Ok((None, None));
    };
    Ok((
        i.checked_sub(1).map(|p| list[p].clone()),
        list.get(i + 1).cloned(),
    ))
}

// ── div-gap: O3 MutationObserver ────────────────────────────────────────

/// div-gap: O3 MutationObserver. A `childList` mutation record. Div has no
/// MutationObserver, so the DOM operations that should queue these go
/// through [`super::AutoAnimate::append_child`],
/// [`super::AutoAnimate::insert_before`] and [`super::AutoAnimate::remove`],
/// which make the change on Div and queue the records the DOM would queue.
#[derive(Debug, Clone)]
pub struct MutationRecord {
    pub target: Element,
    pub added: Vec<Element>,
    pub removed: Vec<Element>,
    pub previous_sibling: Option<Element>,
    pub next_sibling: Option<Element>,
}

/// div-gap: O3 MutationObserver. An observer of one parent's `childList`.
pub(crate) struct Observer {
    pub target: Element,
    pub records: Vec<MutationRecord>,
}

// ── div-gap: F21 HTML timers on the document clock ─────────────────────

/// What a timer runs. AutoAnimate's timer callbacks, named.
#[derive(Clone)]
pub(crate) enum TimerTask {
    /// `updatePos`'s debounced callback.
    UpdatePos(Element),
    /// `cleanUp`'s `setTimeout(..., 0)`: the element's styles to clear.
    CleanUpLater(Element, Option<Vec<&'static str>>),
    /// `poll`'s first timeout, which starts the interval.
    PollStart(Element),
    /// `poll`'s interval.
    PollTick(Element),
}

struct TimerEntry {
    id: u64,
    due: f64,
    interval: Option<f64>,
    task: TimerTask,
}

/// div-gap: F21 HTML timers on the document clock. `setTimeout` and
/// `setInterval`, run by the host's frame at the document's time, in
/// due-time order, then id order. Div's `divui::time::sleep` runs on the
/// wall clock, which the headless frame advance does not move.
pub(crate) struct Timers {
    next_id: u64,
    entries: Vec<TimerEntry>,
}

fn delay(ms: f64) -> f64 {
    if ms.is_nan() {
        0.0
    } else {
        ms.max(0.0)
    }
}

impl Timers {
    pub fn new() -> Self {
        Self {
            next_id: 0,
            entries: Vec::new(),
        }
    }

    pub fn set_timeout(&mut self, now: f64, task: TimerTask, ms: f64) -> u64 {
        self.next_id += 1;
        self.entries.push(TimerEntry {
            id: self.next_id,
            due: now + delay(ms),
            interval: None,
            task,
        });
        self.next_id
    }

    pub fn set_interval(&mut self, now: f64, task: TimerTask, ms: f64) -> u64 {
        self.next_id += 1;
        self.entries.push(TimerEntry {
            id: self.next_id,
            due: now + delay(ms),
            interval: Some(delay(ms)),
            task,
        });
        self.next_id
    }

    pub fn clear(&mut self, id: u64) {
        self.entries.retain(|entry| entry.id != id);
    }

    /// The next timer due at `now`, removed (or rescheduled, for an
    /// interval).
    pub fn take_due(&mut self, now: f64) -> Option<TimerTask> {
        let i = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.due <= now)
            .min_by(|(_, a), (_, b)| a.due.total_cmp(&b.due).then(a.id.cmp(&b.id)))
            .map(|(i, _)| i)?;
        match self.entries[i].interval {
            Some(interval) => {
                self.entries[i].due += interval;
                Some(self.entries[i].task.clone())
            }
            None => Some(self.entries.remove(i).task),
        }
    }
}

// ── div-gap: F22 requestIdleCallback ────────────────────────────────────

/// div-gap: F22 requestIdleCallback. An idle callback; the host's frame
/// runs the queued ones after the frame's other work.
pub(crate) enum IdleTask {
    /// `poll`'s `lowPriority(updatePos)`.
    UpdatePos(Element),
}

// ── Microtasks ──────────────────────────────────────────────────────────

/// The microtasks AutoAnimate's code queues: promise reactions and mutation
/// observer notification. Run at each checkpoint in order.
pub(crate) enum Microtask {
    /// The rest of `updatePos`'s callback after `await animation.finished`.
    UpdatePosContinue(Element),
    /// Notify the mutation observers (div-gap: O3).
    NotifyObservers,
}

pub(crate) type Microtasks = VecDeque<Microtask>;

// ── div-gap: A3 Web Animations API ──────────────────────────────────────

/// What a `finish` event listener does.
pub(crate) enum Listener {
    /// `remain` and `add`: `updatePos(el, false)`.
    UpdatePos(Element),
    /// `remove`: `cleanUp(el, styleReset)`.
    CleanUp(Element, Vec<&'static str>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlayState {
    Running,
    Finished,
    /// Cancelled.
    Idle,
}

/// div-gap: A3 Web Animations API. The `Animation` that
/// `element.animate(keyframes, options)` returns. Div's `Element::animate`
/// animates one property and returns an `AnimationId`, so this holds one
/// per property, and keeps the `playState`, the `finished` promise's
/// reactions and the `finish` event's listeners that Div's id lacks. The
/// animation has finished when Div reports every property's animation
/// finished.
pub(crate) struct Animation {
    pub ids: Vec<AnimationId>,
    pub state: PlayState,
    /// Elements whose `updatePos` awaits `finished`.
    pub waiters: Vec<Element>,
    pub listeners: Vec<Listener>,
}

impl Animation {
    /// Div's view: every property's animation has finished. `Err` when Div
    /// no longer knows one of them (it drops a detached element's
    /// animations).
    pub fn div_finished(&self, document: &Document) -> Result<bool, div::Error> {
        for id in &self.ids {
            if !document.animation_state(*id)?.finished {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
