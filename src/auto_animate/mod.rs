//! A port of AutoAnimate (`@formkit/auto-animate` 0.10.2, `index.mjs`):
//! `autoAnimate(parent, options)` animates the parent's children as they
//! are added, removed and moved, with FLIP for moves.
//!
//! The port keeps AutoAnimate's formulas and operation order: what each
//! mutation means (`getElements`, `animate`), the measurements (`getCoords`,
//! `getTransitionSizes`, `deletePosition`), the keyframes and options of
//! each `element.animate()` call, the re-insertion of removed children with
//! their style reset, and the timers that keep the measurements current.
//! `tools/auto_animate_oracle.py` records what AutoAnimate does in pinned
//! Chromium, and `tests/auto_animate_values.rs` compares this port with it.
//!
//! What it takes from the platform and Div lacks sits behind scaffolds in
//! [`platform`]: MutationObserver (O3), the multi-property Web Animations
//! `Animation` with `finished` and `finish` (A3), HTML timers on the
//! document clock (F21), `requestIdleCallback` (F22) and the viewport size
//! of `document.documentElement` (F23).
//!
//! Not ported: plugins (a function as the options), the ResizeObserver and
//! IntersectionObserver re-measurements (O1, O2; `observePosition`,
//! `handleResizes`, `updateAllPos`), the page scroll correction of removed
//! children (`adjustScroll`, which needs the window's scroll position), the
//! re-measurement after CSS transitions and animations end
//! (`remeasureAfterMove`, F1/F2) and the framework wrappers.

mod platform;

pub use platform::MutationRecord;

use crate::motion::easing::NativeEasing;
use crate::motion::js::{math_round, number_to_string};
use crate::motion::to_div_easing;
use div::prelude::*;
use div::typed_style::{
    parse_literal, AnimationOptions, BoxSizing, FillMode, Keyframe, Keyframes, Position,
};
use platform::{
    children, same, siblings_of, Animation, IdleTask, Listener, Microtask, Microtasks, NodeMap,
    Observer, PlayState, TimerTask, Timers,
};
use std::collections::VecDeque;

/// `autoAnimate`'s options. A plugin (a function) is not ported.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub duration: f64,
    /// A CSS easing keyword: `linear`, `ease`, `ease-in`, `ease-out` or
    /// `ease-in-out`.
    pub easing: String,
    pub disrespect_user_motion_preference: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            duration: 250.0,
            easing: "ease-in-out".into(),
            disrespect_user_motion_preference: false,
        }
    }
}

/// A keyframe value as AutoAnimate writes it.
#[derive(Debug, Clone, PartialEq)]
pub enum KeyValue {
    Str(String),
    Num(f64),
}

/// One keyframe: properties in the order AutoAnimate writes them.
pub type KeyframeSpec = Vec<(&'static str, KeyValue)>;

/// What AutoAnimate did, in order, as the oracle records it in Chromium.
#[derive(Debug, Clone)]
pub enum Event {
    /// `element.animate(keyframes, { duration, easing })`; animations are
    /// numbered in creation order.
    Animate {
        element: Element,
        keyframes: Vec<KeyframeSpec>,
        duration: f64,
        easing: String,
    },
    Cancel(usize),
    Finish(usize),
    /// The animation reached its end on the document clock.
    End(usize),
    /// `el.style[property] = value`.
    Style {
        element: Element,
        property: &'static str,
        value: String,
    },
    InsertBefore {
        parent: Element,
        node: Element,
        child: Element,
    },
    AppendChild {
        parent: Element,
        node: Element,
    },
    Remove(Element),
}

/// `autoAnimate`'s controller.
#[derive(Debug, Clone)]
pub struct Controller {
    pub parent: Element,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Coords {
    top: f64,
    left: f64,
    width: f64,
    height: f64,
}

#[derive(Debug, Clone)]
struct Options {
    duration: f64,
    easing: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            duration: 250.0,
            easing: "ease-in-out".into(),
        }
    }
}

/// The style AutoAnimate gives a removed child while it animates out.
const STYLE_RESET: [&str; 9] = [
    "position",
    "top",
    "left",
    "width",
    "height",
    "margin",
    "pointer-events",
    "transform-origin",
    "z-index",
];

/// AutoAnimate's module state and the scaffolds it runs on.
pub struct AutoAnimate {
    document: Document,
    /// div-gap: F23 `document.documentElement`. Its `clientWidth` and
    /// `clientHeight` (the viewport), which Div's document has no root
    /// element to report. The host passes them.
    viewport: (f64, f64),
    now: f64,
    random: Box<dyn FnMut() -> f64>,

    parents: NodeMap<()>,
    coords: NodeMap<Coords>,
    siblings: NodeMap<(Option<Element>, Option<Element>)>,
    animations: NodeMap<usize>,
    options: NodeMap<Options>,
    debounces: NodeMap<u64>,
    intervals: NodeMap<u64>,
    enabled: NodeMap<()>,
    /// The `__aa_tgt`, `__aa_del` and `__aa_new` marks.
    target: NodeMap<Element>,
    deleting: NodeMap<()>,
    new: NodeMap<()>,

    observers: Vec<Observer>,
    anims: Vec<Animation>,
    timers: Timers,
    idle: Vec<IdleTask>,
    microtasks: Microtasks,
    notify_queued: bool,
    /// Animations finished by `finish()` whose event is not dispatched yet.
    pending_finish: Vec<usize>,

    host_call: bool,
    log: Vec<Event>,
    errors: Vec<String>,
}

impl AutoAnimate {
    /// AutoAnimate loaded in `document`, whose viewport is `viewport`
    /// (div-gap: F23).
    pub fn new(document: &Document, viewport: (f64, f64)) -> Self {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        Self {
            document: document.clone(),
            viewport,
            now: 0.0,
            // `Math.random()` for `poll`'s start offset.
            random: Box::new(move || {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                (seed >> 11) as f64 / (1u64 << 53) as f64
            }),
            parents: NodeMap::new(),
            coords: NodeMap::new(),
            siblings: NodeMap::new(),
            animations: NodeMap::new(),
            options: NodeMap::new(),
            debounces: NodeMap::new(),
            intervals: NodeMap::new(),
            enabled: NodeMap::new(),
            target: NodeMap::new(),
            deleting: NodeMap::new(),
            new: NodeMap::new(),
            observers: Vec::new(),
            anims: Vec::new(),
            timers: Timers::new(),
            idle: Vec::new(),
            microtasks: VecDeque::new(),
            notify_queued: false,
            pending_finish: Vec::new(),
            host_call: false,
            log: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Replace `Math.random()` (the tests fix it, as the oracle does).
    pub fn set_random(&mut self, random: impl FnMut() -> f64 + 'static) {
        self.random = Box::new(random);
    }

    /// What AutoAnimate did since the last call.
    pub fn take_log(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.log)
    }

    /// Div calls that failed, in order.
    pub fn errors(&self) -> &[String] {
        &self.errors
    }

    fn check<T: Default>(&mut self, result: Result<T, div::Error>, what: &str) -> T {
        result.unwrap_or_else(|error| {
            self.errors.push(format!("{what}: {error:?}"));
            T::default()
        })
    }

    // ── The host's side of the event loop ─────────────────────────────

    /// A frame at document time `now` (the host has advanced Div's clock):
    /// the due timers, then the animations that reached their end with
    /// their `finished` reactions and `finish` events, then the idle
    /// callbacks. Each callback is followed by a microtask checkpoint.
    ///
    /// div-gap: F21 HTML timers on the document clock; div-gap: A3 Web
    /// Animations API (the `finish` event); div-gap: F22
    /// requestIdleCallback.
    pub fn frame(&mut self, now: f64) {
        self.now = now;
        let _ = self.document.drain_animation_events();
        while let Some(task) = self.timers.take_due(now) {
            self.run_timer(task);
            self.run_microtasks();
        }
        let mut ended = Vec::new();
        for i in 0..self.anims.len() {
            if self.anims[i].state != PlayState::Running {
                continue;
            }
            let finished = self.anims[i].div_finished(&self.document);
            if self.check(finished, "animation state") {
                self.log.push(Event::End(i));
                self.settle_finished(i);
                ended.push(i);
            }
        }
        self.run_microtasks();
        for i in ended {
            self.dispatch_finish(i);
        }
        for task in std::mem::take(&mut self.idle) {
            match task {
                IdleTask::UpdatePos(element) => self.update_pos(&element, true),
            }
            self.run_microtasks();
        }
    }

    /// After the host's calls in a frame: the microtask checkpoint (mutation
    /// observers, promise reactions), then the `finish` events of
    /// animations that `finish()` ended.
    pub fn checkpoint(&mut self) {
        self.run_microtasks();
        for i in std::mem::take(&mut self.pending_finish) {
            self.dispatch_finish(i);
        }
    }

    fn run_microtasks(&mut self) {
        while let Some(task) = self.microtasks.pop_front() {
            match task {
                Microtask::UpdatePosContinue(element) => {
                    // `coords.set(el, getCoords(el)); observePosition(el)`.
                    // observePosition only (re)creates an
                    // IntersectionObserver (O2, not ported).
                    let coords = self.get_coords(&element);
                    self.coords.set(&element, coords);
                }
                Microtask::NotifyObservers => {
                    self.notify_queued = false;
                    for i in 0..self.observers.len() {
                        let Some(observer) = self.observers.get_mut(i) else {
                            break;
                        };
                        let records = std::mem::take(&mut observer.records);
                        if !records.is_empty() {
                            self.handle_mutations(&records);
                        }
                    }
                }
            }
        }
    }

    fn run_timer(&mut self, task: TimerTask) {
        match task {
            TimerTask::UpdatePos(element) => {
                // `const currentAnimation = animations.get(el);
                // await currentAnimation?.finished; ...`
                match self.animations.get(&element).map(|i| self.anims[*i].state) {
                    None | Some(PlayState::Finished) => self
                        .microtasks
                        .push_back(Microtask::UpdatePosContinue(element)),
                    Some(PlayState::Running) => {
                        let i = *self.animations.get(&element).unwrap();
                        self.anims[i].waiters.push(element);
                    }
                    // A cancelled animation's new `finished` never settles.
                    Some(PlayState::Idle) => {}
                }
            }
            TimerTask::CleanUpLater(element, styles) => {
                self.deleting.delete(&element);
                self.new.set(&element, ());
                if let Some(styles) = styles {
                    for property in styles {
                        self.write_style(&element, property, String::new());
                    }
                }
            }
            TimerTask::PollStart(element) => {
                let id = self.timers.set_interval(
                    self.now,
                    TimerTask::PollTick(element.clone()),
                    2000.0,
                );
                self.intervals.set(&element, id);
            }
            TimerTask::PollTick(element) => {
                // `lowPriority(updatePos.bind(null, el))`.
                self.idle.push(IdleTask::UpdatePos(element));
            }
        }
    }

    // ── div-gap: A3 Web Animations API ───────────────────────────────

    /// `el.animate(keyframes, { duration, easing })`.
    fn element_animate(
        &mut self,
        element: &Element,
        keyframes: Vec<KeyframeSpec>,
        duration: f64,
        easing: &str,
    ) -> usize {
        let index = self.anims.len();
        self.log.push(Event::Animate {
            element: element.clone(),
            keyframes: keyframes.clone(),
            duration,
            easing: easing.to_string(),
        });
        let mut ids = Vec::new();
        match div_easing(easing) {
            Some(div_easing) => {
                let options = AnimationOptions {
                    duration_ms: duration,
                    fill: FillMode::None,
                    easing: div_easing,
                    ..AnimationOptions::default()
                };
                let offsets = offsets(&keyframes);
                for (property, _) in keyframes[0].iter().filter(|(name, _)| *name != "offset") {
                    match div_keyframes(property, &keyframes, &offsets).and_then(
                        |(property_id, frames)| {
                            element
                                .animate(property_id, frames, options.clone())
                                .map_err(|error| format!("{error:?}"))
                        },
                    ) {
                        Ok(id) => ids.push(id),
                        Err(error) => self
                            .errors
                            .push(format!("animate {property} on an element: {error}")),
                    }
                }
            }
            None => self.errors.push(format!("easing {easing:?} is not ported")),
        }
        self.anims.push(Animation {
            ids,
            state: PlayState::Running,
            waiters: Vec::new(),
            listeners: Vec::new(),
        });
        index
    }

    /// `animation.cancel()`.
    fn cancel(&mut self, i: usize) {
        self.log.push(Event::Cancel(i));
        for id in self.anims[i].ids.clone() {
            // A finished or already cancelled animation may be gone.
            let _ = self.document.cancel_animation(id);
        }
        let animation = &mut self.anims[i];
        animation.state = PlayState::Idle;
        animation.waiters.clear();
        animation.listeners.clear();
    }

    /// `animation.finish()`.
    fn finish(&mut self, i: usize) {
        self.log.push(Event::Finish(i));
        for id in self.anims[i].ids.clone() {
            let _ = self.document.finish_animation(id);
        }
        self.settle_finished(i);
        self.pending_finish.push(i);
    }

    /// The animation finished: resolve `finished`.
    fn settle_finished(&mut self, i: usize) {
        self.anims[i].state = PlayState::Finished;
        for element in std::mem::take(&mut self.anims[i].waiters) {
            self.microtasks
                .push_back(Microtask::UpdatePosContinue(element));
        }
    }

    /// Dispatch the `finish` event to its (`once`) listeners.
    fn dispatch_finish(&mut self, i: usize) {
        for listener in std::mem::take(&mut self.anims[i].listeners) {
            match listener {
                Listener::UpdatePos(element) => self.update_pos(&element, false),
                Listener::CleanUp(element, styles) => self.clean_up(&element, Some(styles)),
            }
            self.run_microtasks();
        }
    }

    // ── div-gap: O3 MutationObserver ─────────────────────────────────

    fn queue_record(&mut self, record: MutationRecord) {
        let mut queued = false;
        for observer in &mut self.observers {
            if same(&observer.target, &record.target) {
                observer.records.push(record.clone());
                queued = true;
            }
        }
        if queued && !self.notify_queued {
            self.notify_queued = true;
            self.microtasks.push_back(Microtask::NotifyObservers);
        }
    }

    /// The removal record of `node` from its current parent, if it has one.
    fn removal_record(node: &Element) -> Result<Option<MutationRecord>, div::Error> {
        let Some(parent) = node.parent()? else {
            return Ok(None);
        };
        let (previous_sibling, next_sibling) = siblings_of(node)?;
        Ok(Some(MutationRecord {
            target: parent,
            added: Vec::new(),
            removed: vec![node.clone()],
            previous_sibling,
            next_sibling,
        }))
    }

    /// `node.remove()`: Div's `detach` keeps the node for re-insertion.
    fn queue_removal(&mut self, node: &Element) -> Result<(), div::Error> {
        if let Some(record) = Self::removal_record(node)? {
            node.detach()?;
            self.queue_record(record);
        }
        Ok(())
    }

    fn dom_insert(
        &mut self,
        parent: &Element,
        node: &Element,
        child: Option<&Element>,
    ) -> Result<(), div::Error> {
        // Moving an attached node queues its removal first, as the DOM does.
        // Div moves it without detaching, which keeps its animations.
        if let Some(record) = Self::removal_record(node)? {
            self.queue_record(record);
        }
        match child {
            Some(child) => parent.insert_before(node, child)?,
            None => parent.append_child(node)?,
        }
        let (previous_sibling, next_sibling) = siblings_of(node)?;
        self.queue_record(MutationRecord {
            target: parent.clone(),
            added: vec![node.clone()],
            removed: Vec::new(),
            previous_sibling,
            next_sibling,
        });
        Ok(())
    }

    /// The host's `parent.appendChild(node)`. div-gap: O3
    /// MutationObserver: it queues the mutation records the DOM would.
    pub fn append_child(&mut self, parent: &Element, node: &Element) -> Result<(), div::Error> {
        self.host_call = true;
        let result = self.dom_insert(parent, node, None);
        self.host_call = false;
        result
    }

    /// The host's `parent.insertBefore(node, child)`. div-gap: O3.
    pub fn insert_before(
        &mut self,
        parent: &Element,
        node: &Element,
        child: &Element,
    ) -> Result<(), div::Error> {
        self.host_call = true;
        let result = self.dom_insert(parent, node, Some(child));
        self.host_call = false;
        result
    }

    /// The host's `node.remove()`. div-gap: O3.
    pub fn remove(&mut self, node: &Element) -> Result<(), div::Error> {
        self.host_call = true;
        let result = self.queue_removal(node);
        self.host_call = false;
        result
    }

    /// AutoAnimate's own `insertBefore`/`appendChild`.
    fn aa_insert(&mut self, parent: &Element, node: &Element, child: Option<&Element>) {
        if !self.host_call {
            self.log.push(match child {
                Some(child) => Event::InsertBefore {
                    parent: parent.clone(),
                    node: node.clone(),
                    child: child.clone(),
                },
                None => Event::AppendChild {
                    parent: parent.clone(),
                    node: node.clone(),
                },
            });
        }
        let result = self.dom_insert(parent, node, child);
        self.check(result, "insert");
    }

    // ── Style, geometry ──────────────────────────────────────────────

    /// `el.style[property] = value`; `""` removes the declaration.
    fn write_style(&mut self, element: &Element, property: &'static str, value: String) {
        self.log.push(Event::Style {
            element: element.clone(),
            property,
            value: value.clone(),
        });
        let Some(property_id) = StyleProperty::from_css_name(property) else {
            self.errors.push(format!("no Div property {property}"));
            return;
        };
        let result = if value.is_empty() {
            element
                .remove_property(property_id)
                .map(|_| ())
                .map_err(|error| format!("{error:?}"))
        } else {
            parse_literal(property_id, &value)
                .map_err(|error| format!("{error:?}"))
                .and_then(|parsed| {
                    element
                        .set_property(property_id, parsed)
                        .map_err(|error| format!("{error:?}"))
                })
        };
        if let Err(error) = result {
            self.errors.push(format!("{property}: {value:?}: {error}"));
        }
    }

    /// `getScrollOffset(el)`.
    fn scroll_offset(&mut self, element: &Element) -> (f64, f64) {
        let mut parent = element.parent().unwrap_or(None);
        while let Some(p) = parent {
            let left = p.scroll_left().unwrap_or(0.0);
            let top = p.scroll_top().unwrap_or(0.0);
            if left != 0.0 || top != 0.0 {
                return (left, top);
            }
            parent = p.parent().unwrap_or(None);
        }
        (0.0, 0.0)
    }

    /// `getCoords(el)`: the border box from `getBoundingClientRect`, all
    /// zero for an element without a box, plus the scroll offset.
    fn get_coords(&mut self, element: &Element) -> Coords {
        let rect = element.bounding_rect();
        let rect = self.check(
            rect.map(|r| r.map(|r| (r.x, r.y, r.width, r.height))),
            "rect",
        );
        let (x, y, width, height) = rect.unwrap_or_default();
        let (sx, sy) = self.scroll_offset(element);
        Coords {
            top: f64::from(y) + sy,
            left: f64::from(x) + sx,
            width: f64::from(width),
            height: f64::from(height),
        }
    }

    /// `getTransitionSizes(el, oldCoords, newCoords)`.
    fn transition_sizes(&mut self, element: &Element, old: Coords, new: Coords) -> [f64; 4] {
        let (mut width_from, mut height_from) = (old.width, old.height);
        let (mut width_to, mut height_to) = (new.width, new.height);
        match element.computed_style() {
            Ok(style) if style.box_sizing == BoxSizing::ContentBox => {
                let mut px = |length: &div::typed_style::Length| {
                    if length.is_fixed() {
                        f64::from(length.value())
                    } else {
                        self.errors
                            .push("a padding that is not in px is not ported".into());
                        0.0
                    }
                };
                let padding_y = px(&style.padding_top)
                    + px(&style.padding_bottom)
                    + f64::from(style.effective_border_top())
                    + f64::from(style.effective_border_bottom());
                let padding_x = px(&style.padding_left)
                    + px(&style.padding_right)
                    + f64::from(style.effective_border_right())
                    + f64::from(style.effective_border_left());
                width_from -= padding_x;
                width_to -= padding_x;
                height_from -= padding_y;
                height_to -= padding_y;
            }
            Ok(_) => {}
            Err(error) => self.errors.push(format!("computed style: {error:?}")),
        }
        [width_from, width_to, height_from, height_to].map(math_round)
    }

    fn get_options(&self, element: &Element) -> Options {
        self.target
            .get(element)
            .and_then(|target| self.options.get(target))
            .cloned()
            .unwrap_or_default()
    }

    fn is_enabled(&self, element: &Element) -> bool {
        self.target
            .get(element)
            .is_some_and(|target| self.enabled.has(target))
    }

    /// `isOffscreen(el)`, against the viewport (div-gap: F23).
    fn is_offscreen(&mut self, element: &Element) -> bool {
        let rect = element.bounding_rect();
        let rect = self.check(
            rect.map(|r| r.map(|r| (r.x, r.y, r.width, r.height))),
            "rect",
        );
        let (x, y, width, height) = rect.unwrap_or_default();
        let (x, y) = (f64::from(x), f64::from(y));
        let (right, bottom) = (x + f64::from(width), y + f64::from(height));
        let (vw, vh) = self.viewport;
        bottom < 0.0 || y > vh || right < 0.0 || x > vw
    }

    /// `forEach(parent, ...callbacks)`: the parent, then its children.
    fn parent_and_children(&mut self, parent: &Element) -> Vec<Element> {
        let list = children(parent);
        let mut out = vec![parent.clone()];
        out.extend(self.check(list, "children"));
        out
    }

    /// `updatePos(el, debounce)`.
    fn update_pos(&mut self, element: &Element, debounce: bool) {
        if let Some(id) = self.debounces.get(element) {
            self.timers.clear(*id);
        }
        let delay = if debounce {
            self.get_options(element).duration
        } else {
            0.0
        };
        let id = self
            .timers
            .set_timeout(self.now, TimerTask::UpdatePos(element.clone()), delay);
        self.debounces.set(element, id);
    }

    /// `poll(el)`.
    fn poll(&mut self, element: &Element) {
        let offset = math_round(2000.0 * (self.random)());
        self.timers
            .set_timeout(self.now, TimerTask::PollStart(element.clone()), offset);
    }

    // ── AutoAnimate ──────────────────────────────────────────────────

    /// `handleMutations(mutations)`.
    fn handle_mutations(&mut self, records: &[MutationRecord]) {
        if let Some(elements) = self.get_elements(records) {
            for element in elements {
                self.animate(&element);
            }
        }
    }

    /// `target(el, child)`: mark the target once.
    fn mark_target(&mut self, element: &Element, child: Option<&Element>) {
        match child {
            None if !self.target.has(element) => self.target.set(element, element.clone()),
            Some(child) if !self.target.has(child) => self.target.set(child, element.clone()),
            _ => {}
        }
    }

    /// `getElements(mutations)`: `None` when the mutations are ignored.
    fn get_elements(&mut self, records: &[MutationRecord]) -> Option<Vec<Element>> {
        // Only comment nodes (or none) observed: ignored. The ported pages
        // have element children only.
        if records
            .iter()
            .all(|record| record.added.is_empty() && record.removed.is_empty())
        {
            return None;
        }
        let mut elements: Vec<Element> = Vec::new();
        let contains = |list: &[Element], element: &Element| list.iter().any(|e| same(e, element));
        for record in records {
            let target = &record.target;
            self.mark_target(target, None);
            if !contains(&elements, target) {
                elements.push(target.clone());
                let list = children(target);
                for child in self.check(list, "children") {
                    if self.deleting.has(&child) {
                        return None;
                    }
                    self.mark_target(target, Some(&child));
                    if !contains(&elements, &child) {
                        elements.push(child);
                    }
                }
            }
            for child in &record.removed {
                if self.deleting.has(child) {
                    return None;
                }
                if !contains(&elements, child) {
                    elements.push(child.clone());
                }
                self.mark_target(target, Some(child));
                self.siblings.set(
                    child,
                    (record.previous_sibling.clone(), record.next_sibling.clone()),
                );
            }
        }
        Some(elements)
    }

    /// `animate(el)`.
    fn animate(&mut self, element: &Element) {
        let mounted = element.is_connected();
        let mounted = self.check(mounted, "isConnected");
        let pre_existing = self.coords.has(element);
        if mounted && self.siblings.has(element) {
            self.siblings.delete(element);
        }
        if let Some(i) = self.animations.get(element).copied() {
            if self.anims[i].state != PlayState::Finished {
                self.cancel(i);
            }
        }
        if self.new.has(element) {
            self.add(element);
        } else if pre_existing && mounted {
            self.remain(element);
        } else if pre_existing && !mounted {
            self.remove_animated(element);
        } else {
            self.add(element);
        }
    }

    /// `remain(el)`: FLIP from the old coordinates.
    fn remain(&mut self, element: &Element) {
        let old = self.coords.get(element).copied();
        let new = self.get_coords(element);
        if !self.is_enabled(element) {
            self.coords.set(element, new);
            return;
        }
        if self.is_offscreen(element) {
            self.coords.set(element, new);
            return;
        }
        let Some(old) = old else {
            return;
        };
        let options = self.get_options(element);
        let mut delta_left = old.left - new.left;
        let mut delta_top = old.top - new.top;
        let delta_right = old.left + old.width - (new.left + new.width);
        let delta_bottom = old.top + old.height - (new.top + new.height);
        if delta_bottom == 0.0 {
            delta_top = 0.0;
        }
        if delta_right == 0.0 {
            delta_left = 0.0;
        }
        let [width_from, width_to, height_from, height_to] =
            self.transition_sizes(element, old, new);
        let mut start: KeyframeSpec = vec![(
            "transform",
            KeyValue::Str(format!(
                "translate({}px, {}px)",
                number_to_string(delta_left),
                number_to_string(delta_top)
            )),
        )];
        let mut end: KeyframeSpec = vec![("transform", KeyValue::Str("translate(0, 0)".into()))];
        if width_from != width_to {
            start.push(("width", px(width_from)));
            end.push(("width", px(width_to)));
        }
        if height_from != height_to {
            start.push(("height", px(height_from)));
            end.push(("height", px(height_to)));
        }
        let animation =
            self.element_animate(element, vec![start, end], options.duration, &options.easing);
        self.animations.set(element, animation);
        self.coords.set(element, new);
        self.anims[animation]
            .listeners
            .push(Listener::UpdatePos(element.clone()));
    }

    /// `add(el)`: fade and scale in.
    fn add(&mut self, element: &Element) {
        self.new.delete(element);
        let coords = self.get_coords(element);
        self.coords.set(element, coords);
        let options = self.get_options(element);
        if !self.is_enabled(element) {
            return;
        }
        if self.is_offscreen(element) {
            return;
        }
        let animation = self.element_animate(
            element,
            vec![
                vec![
                    ("transform", KeyValue::Str("scale(.98)".into())),
                    ("opacity", KeyValue::Num(0.0)),
                ],
                vec![
                    ("transform", KeyValue::Str("scale(0.98)".into())),
                    ("opacity", KeyValue::Num(0.0)),
                    ("offset", KeyValue::Num(0.5)),
                ],
                vec![
                    ("transform", KeyValue::Str("scale(1)".into())),
                    ("opacity", KeyValue::Num(1.0)),
                ],
            ],
            options.duration * 1.5,
            "ease-in",
        );
        self.animations.set(element, animation);
        self.anims[animation]
            .listeners
            .push(Listener::UpdatePos(element.clone()));
    }

    /// `cleanUp(el, styles)`.
    fn clean_up(&mut self, element: &Element, styles: Option<Vec<&'static str>>) {
        if !self.host_call {
            self.log.push(Event::Remove(element.clone()));
        }
        let result = self.queue_removal(element);
        self.check(result, "remove");
        self.coords.delete(element);
        self.siblings.delete(element);
        self.animations.delete(element);
        self.timers.set_timeout(
            self.now,
            TimerTask::CleanUpLater(element.clone(), styles),
            0.0,
        );
    }

    /// `remove(el)`: put the removed child back, absolutely positioned
    /// where it was, and fade and scale it out.
    fn remove_animated(&mut self, element: &Element) {
        if !self.siblings.has(element) || !self.coords.has(element) {
            return;
        }
        let (previous, next) = self.siblings.get(element).cloned().unwrap();
        self.deleting.set(element, ());
        // `adjustScroll` (window scroll correction) is not ported.
        let next_parent = next.as_ref().map(|n| n.parent().unwrap_or(None));
        let previous_parent = previous.as_ref().map(|p| p.parent().unwrap_or(None));
        if let (Some(next), Some(Some(parent))) = (&next, next_parent) {
            self.aa_insert(&parent, element, Some(next));
        } else if let Some(Some(parent)) = previous_parent {
            self.aa_insert(&parent, element, None);
        } else if let Some(target) = self.target.get(element).cloned() {
            self.aa_insert(&target, element, None);
        }
        if !self.is_enabled(element) {
            self.clean_up(element, None);
            return;
        }
        let [top, left, width, height] = self.delete_position(element);
        let options = self.get_options(element);
        let reset = [
            "absolute".to_string(),
            format!("{}px", number_to_string(top)),
            format!("{}px", number_to_string(left)),
            format!("{}px", number_to_string(width)),
            format!("{}px", number_to_string(height)),
            "0".into(),
            "none".into(),
            "center".into(),
            "100".into(),
        ];
        for (property, value) in STYLE_RESET.into_iter().zip(reset) {
            self.write_style(element, property, value);
        }
        let animation = self.element_animate(
            element,
            vec![
                vec![
                    ("transform", KeyValue::Str("scale(1)".into())),
                    ("opacity", KeyValue::Num(1.0)),
                ],
                vec![
                    ("transform", KeyValue::Str("scale(.98)".into())),
                    ("opacity", KeyValue::Num(0.0)),
                ],
            ],
            options.duration,
            "ease-out",
        );
        self.animations.set(element, animation);
        self.anims[animation]
            .listeners
            .push(Listener::CleanUp(element.clone(), STYLE_RESET.to_vec()));
    }

    /// `deletePosition(el)`: `[top, left, width, height]`.
    fn delete_position(&mut self, element: &Element) -> [f64; 4] {
        let old = *self.coords.get(element).unwrap();
        let now = self.get_coords(element);
        let [width, _, height, _] = self.transition_sizes(element, old, now);
        let body = self.document.body();
        let mut offset_parent = element.parent().unwrap_or(None);
        while let Some(parent) = offset_parent.clone() {
            let position = parent.computed_style().map(|style| style.position);
            if position.map_or(true, |p| p == Position::Static) || same(&parent, &body) {
                offset_parent = parent.parent().unwrap_or(None);
            } else {
                break;
            }
        }
        let offset_parent = offset_parent.unwrap_or(body);
        let finished_or_none = self
            .animations
            .get(element)
            .is_none_or(|i| self.anims[*i].state == PlayState::Finished);
        let parent_coords = if finished_or_none {
            self.get_coords(&offset_parent)
        } else {
            match self.coords.get(&offset_parent).copied() {
                Some(coords) => coords,
                None => self.get_coords(&offset_parent),
            }
        };
        let (border_top, border_left) = match offset_parent.computed_style() {
            Ok(style) => (
                f64::from(style.effective_border_top()),
                f64::from(style.effective_border_left()),
            ),
            Err(_) => (0.0, 0.0),
        };
        let top = math_round(old.top - parent_coords.top) - border_top;
        let left = math_round(old.left - parent_coords.left) - border_left;
        [top, left, width, height]
    }

    // ── The controller ───────────────────────────────────────────────

    /// `autoAnimate(el, config)`.
    pub fn auto_animate(&mut self, parent: &Element, config: Config) -> Controller {
        let reduced = self.document.prefers_reduced_motion().unwrap_or(false)
            && !config.disrespect_user_motion_preference;
        if !reduced {
            self.enabled.set(parent, ());
            if matches!(
                parent.computed_style().map(|s| s.position),
                Ok(Position::Static)
            ) {
                self.write_style(parent, "position", "relative".into());
            }
            for node in self.parent_and_children(parent) {
                // `forEach(el, updatePos, poll, resize.observe)`: each is
                // called with `(node, options.has(node))`.
                let has_options = self.options.has(&node);
                self.update_pos(&node, has_options);
                self.poll(&node);
            }
            self.options.set(
                parent,
                Options {
                    duration: config.duration,
                    easing: config.easing,
                },
            );
            if !self.observers.iter().any(|o| same(&o.target, parent)) {
                self.observers.push(Observer {
                    target: parent.clone(),
                    records: Vec::new(),
                });
            }
            self.parents.set(parent, ());
        }
        Controller {
            parent: parent.clone(),
        }
    }

    /// `controller.enable()`.
    pub fn enable(&mut self, controller: &Controller) {
        self.enabled.set(&controller.parent, ());
    }

    /// `controller.isEnabled()`.
    pub fn is_parent_enabled(&self, controller: &Controller) -> bool {
        self.enabled.has(&controller.parent)
    }

    /// `controller.disable()`.
    pub fn disable(&mut self, controller: &Controller) {
        self.enabled.delete(&controller.parent);
        for node in self.parent_and_children(&controller.parent) {
            if let Some(i) = self.animations.get(&node).copied() {
                if self.deleting.has(&node) {
                    self.finish(i);
                } else {
                    self.cancel(i);
                }
            }
            self.animations.delete(&node);
            if let Some(id) = self.debounces.delete(&node) {
                self.timers.clear(id);
            }
            if let Some(id) = self.intervals.delete(&node) {
                self.timers.clear(id);
            }
        }
    }

    /// `controller.destroy()`.
    pub fn destroy(&mut self, controller: &Controller) {
        let parent = &controller.parent;
        self.enabled.delete(parent);
        self.parents.delete(parent);
        self.options.delete(parent);
        self.observers.retain(|o| !same(&o.target, parent));
        for node in self.parent_and_children(parent) {
            if let Some(i) = self.animations.get(&node).copied() {
                self.cancel(i);
            }
            self.animations.delete(&node);
            if let Some(id) = self.intervals.delete(&node) {
                self.timers.clear(id);
            }
            if let Some(id) = self.debounces.delete(&node) {
                self.timers.clear(id);
            }
            self.coords.delete(&node);
            self.siblings.delete(&node);
        }
    }

    /// The parents AutoAnimate animates.
    pub fn parents(&self) -> Vec<Element> {
        self.parents.keys()
    }
}

fn px(value: f64) -> KeyValue {
    KeyValue::Str(format!("{}px", number_to_string(value)))
}

/// A CSS easing keyword as Div's easing. Other easings are not ported.
fn div_easing(easing: &str) -> Option<div::typed_style::Easing> {
    let keyword = match easing {
        "linear" => "linear",
        "ease" => "ease",
        "ease-in" => "ease-in",
        "ease-out" => "ease-out",
        "ease-in-out" => "ease-in-out",
        _ => return None,
    };
    Some(to_div_easing(&NativeEasing::Keyword(keyword)))
}

/// The keyframes' computed offsets: an explicit `offset`, else 0 for the
/// first, 1 for the last, and evenly spaced between known neighbours
/// (WAAPI's "compute missing keyframe offsets").
fn offsets(keyframes: &[KeyframeSpec]) -> Vec<f64> {
    let n = keyframes.len();
    let mut known: Vec<Option<f64>> = keyframes
        .iter()
        .map(|frame| {
            frame.iter().find_map(|(name, value)| match (*name, value) {
                ("offset", KeyValue::Num(offset)) => Some(*offset),
                _ => None,
            })
        })
        .collect();
    if n > 1 && known[0].is_none() {
        known[0] = Some(0.0);
    }
    if known[n - 1].is_none() {
        known[n - 1] = Some(1.0);
    }
    let mut out: Vec<f64> = known.iter().map(|o| o.unwrap_or(f64::NAN)).collect();
    let mut a = 0;
    for (b, offset) in known.iter().enumerate().skip(1) {
        if let Some(b_offset) = *offset {
            let a_offset = out[a];
            for (j, slot) in out.iter_mut().enumerate().take(b).skip(a + 1) {
                *slot = a_offset + (b_offset - a_offset) * (j - a) as f64 / (b - a) as f64;
            }
            a = b;
        }
    }
    out
}

/// One property's keyframes as Div takes them, parsed with Div's CSS value
/// parser.
fn div_keyframes(
    property: &str,
    keyframes: &[KeyframeSpec],
    offsets: &[f64],
) -> Result<(StyleProperty, Keyframes<StyleValue>), String> {
    let property_id = StyleProperty::from_css_name(property)
        .ok_or_else(|| format!("no Div property {property}"))?;
    let frames = keyframes
        .iter()
        .zip(offsets)
        .map(|(frame, offset)| {
            let value = frame
                .iter()
                .find(|(name, _)| *name == property)
                .map(|(_, value)| match value {
                    KeyValue::Str(s) => s.clone(),
                    KeyValue::Num(n) => number_to_string(*n),
                })
                .ok_or_else(|| format!("{property} missing from a keyframe"))?;
            let parsed = parse_literal(property_id, &value)
                .map_err(|error| format!("{value:?}: {error:?}"))?;
            Ok(Keyframe::new(*offset, parsed))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let frames = Keyframes::new(frames).map_err(|error| format!("{error:?}"))?;
    Ok((property_id, frames))
}
