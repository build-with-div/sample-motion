//! The engine, timers and rendering: `engine/engine.js`, `core/clock.js`,
//! `timer/timer.js` and `core/render.js`.

use super::ease::EaseFn;
use super::value::{
    build_transform_string, clamp, lerp, round, to_int32, Obj, Raw, Transforms, Value, ValueType,
    MIN_VALUE,
};
use super::{StyleHost, StyleWrite, TargetId};
use crate::motion::js::number_to_string;
use std::collections::HashMap;

/// `globals.tickThreshold`.
const TICK_THRESHOLD: f64 = 200.0;
/// `globals.precision`.
const PRECISION: i32 = 4;
/// `maxFps`.
const MAX_FPS: f64 = 240.0;

/// An animation or a timeline (`Timer`, `JSAnimation`, `Timeline`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimerId(pub(crate) usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TweenId(pub(crate) usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TickMode {
    Auto,
    Force,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Composition {
    Replace,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TweenType {
    Css,
    Transform,
}

/// `Clock`'s fields.
#[derive(Debug, Clone)]
pub(crate) struct Clock {
    pub current_time: f64,
    pub last_tick_time: f64,
    pub start_time: f64,
    pub frame_duration: f64,
    pub fps: f64,
    pub speed: f64,
}

impl Clock {
    fn new(init_time: f64) -> Self {
        Self {
            current_time: init_time,
            last_tick_time: init_time,
            start_time: init_time,
            frame_duration: 1000.0 / MAX_FPS,
            fps: MAX_FPS,
            speed: 1.0,
        }
    }

    /// `requestTick(time)`: whether a frame at `time` ticks.
    fn request_tick(&mut self, time: f64) -> bool {
        let frame_duration = self.frame_duration;
        let elapsed = time - self.last_tick_time;
        let scaled = frame_duration * 0.25;
        let tolerance = if scaled < 4.0 { scaled } else { 4.0 };
        if elapsed + tolerance < frame_duration {
            return false;
        }
        self.last_tick_time = if elapsed >= frame_duration {
            time - elapsed % frame_duration
        } else {
            time
        };
        true
    }
}

#[derive(Clone)]
pub(crate) struct Timer {
    pub labels: HashMap<String, f64>,
    pub tl_composition: bool,
    pub id: f64,
    pub parent: Option<TimerId>,
    pub duration: f64,
    pub backwards: bool,
    pub paused: bool,
    pub began: bool,
    pub completed: bool,
    pub iteration_duration: f64,
    pub iteration_count: f64,
    pub autoplay: bool,
    pub offset: f64,
    pub delay: f64,
    pub loop_delay: f64,
    pub iteration_time: f64,
    pub current_iteration: f64,
    pub running: bool,
    pub reversed: i32,
    pub reverse: i32,
    pub cancelled: bool,
    pub alternate: bool,
    pub prev: Option<TimerId>,
    pub next: Option<TimerId>,
    /// A timeline's children.
    pub head: Option<TimerId>,
    pub tail: Option<TimerId>,
    /// An animation's tweens.
    pub tween_head: Option<TweenId>,
    pub tween_tail: Option<TweenId>,
    pub has_children: bool,
    pub clock: Clock,
    pub priority: f64,
    /// The playback ease (`_ease`).
    pub ease: Option<EaseFn>,
    /// `defaults`: a timeline's children read their defaults here.
    pub defaults: Obj,
    pub targets: Vec<TargetId>,
}

pub(crate) struct Tween {
    pub parent: TimerId,
    pub property: String,
    pub target: TargetId,
    pub value: Raw,
    pub ease: EaseFn,
    pub from_number: f64,
    pub to_number: f64,
    pub number: f64,
    pub unit: Option<String>,
    pub current_time: f64,
    pub start_time: f64,
    pub delay: f64,
    pub update_duration: f64,
    pub change_duration: f64,
    pub absolute_start_time: f64,
    pub absolute_update_start_time: f64,
    pub absolute_end_time: f64,
    pub has_from_value: bool,
    pub tween_type: TweenType,
    pub value_type: ValueType,
    pub composition: Composition,
    pub is_overlapped: bool,
    pub is_overridden: bool,
    pub render_transforms: bool,
    pub prev_rep: Option<TweenId>,
    pub next_rep: Option<TweenId>,
    pub prev: Option<TweenId>,
    pub next: Option<TweenId>,
}

/// A target's state: anime.js's transform cache and, standing in for the
/// element's CSSOM inline style, the strings anime.js last wrote.
#[derive(Debug, Default)]
pub(crate) struct Target {
    pub transforms: Transforms,
    /// div-gap: F19 CSSOM inline style serialization. anime.js reads
    /// `el.style.transform` and `el.style.opacity` back when an animation
    /// starts from the current value. Div has no CSSOM string read of an
    /// inline style, so the port reads back the string it wrote. Chromium
    /// returns its own serialization, which differs for numbers with more
    /// than six significant digits (docs/framework-gaps.md).
    pub inline: HashMap<String, String>,
}

/// The engine (`engine/engine.js`): the clock and the running timers.
pub(crate) struct Engine {
    pub clock: Clock,
    pub head: Option<TimerId>,
    pub tail: Option<TimerId>,
    pub has_children: bool,
    /// `reqId`: whether `tickEngine` is queued with requestAnimationFrame.
    pub req_id: bool,
}

/// Where a timer sits: the engine's list or a timeline's children.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Owner {
    Engine,
    Timer(TimerId),
}

/// anime.js's module state, with the host page the styles go to.
pub struct Runtime {
    /// `Date.now()`.
    pub(crate) now: f64,
    pub(crate) engine: Engine,
    pub(crate) timers: Vec<Timer>,
    pub(crate) tweens: Vec<Tween>,
    pub(crate) targets: Vec<Target>,
    /// `lookups._rep`: the replace-composition siblings of a target's
    /// property, head and tail.
    pub(crate) rep: HashMap<(TargetId, String), (Option<TweenId>, Option<TweenId>)>,
    pub(crate) host: Box<dyn StyleHost>,
    /// Animation frame callbacks queued with requestAnimationFrame.
    pub(crate) raf_callbacks: usize,
    pub(crate) timer_id: f64,
    pub(crate) js_animation_id: f64,
    pub(crate) tl_id: f64,
    pub(crate) writes: Vec<StyleWrite>,
}

impl Runtime {
    /// anime.js loaded at `Date.now() == now`: the engine's clock starts.
    pub fn new(host: Box<dyn StyleHost>, now: f64) -> Self {
        Self {
            now,
            engine: Engine {
                clock: Clock::new(now),
                head: None,
                tail: None,
                has_children: false,
                req_id: false,
            },
            timers: Vec::new(),
            tweens: Vec::new(),
            targets: Vec::new(),
            rep: HashMap::new(),
            host,
            raf_callbacks: 0,
            timer_id: 0.0,
            js_animation_id: 0.0,
            tl_id: 0.0,
            writes: Vec::new(),
        }
    }

    /// `Date.now()` for the calls that follow.
    pub fn set_time(&mut self, now: f64) {
        self.now = now;
    }

    /// Whether the engine has a frame callback queued.
    pub fn wants_frame(&self) -> bool {
        self.raf_callbacks > 0
    }

    /// The style writes made since the last call, in order.
    pub fn take_writes(&mut self) -> Vec<StyleWrite> {
        std::mem::take(&mut self.writes)
    }

    /// The inline style string anime.js last wrote for `property`.
    pub fn inline_style(&self, target: TargetId, property: &str) -> Option<&str> {
        self.targets
            .get(target.0)
            .and_then(|t| t.inline.get(property))
            .map(String::as_str)
    }

    /// div-gap: R1 requestAnimationFrame. Run the animation frame callbacks
    /// anime.js queued with `requestAnimationFrame`, with `Date.now()` at
    /// `now`. Callbacks queued while they run wait for the next frame.
    pub fn animation_frame(&mut self, now: f64) {
        self.now = now;
        let callbacks = std::mem::take(&mut self.raf_callbacks);
        for _ in 0..callbacks {
            self.tick_engine();
        }
    }

    pub(crate) fn target_mut(&mut self, target: TargetId) -> &mut Target {
        if self.targets.len() <= target.0 {
            self.targets.resize_with(target.0 + 1, Target::default);
        }
        &mut self.targets[target.0]
    }

    fn write_style(&mut self, target: TargetId, property: &str, value: String) {
        self.target_mut(target)
            .inline
            .insert(property.to_string(), value.clone());
        self.host.set_style(target, property, &value);
        self.writes.push(StyleWrite {
            target,
            property: property.to_string(),
            value,
        });
    }

    // ---- Linked lists (`addChild`, `removeChild`) ----

    fn owner_ends(&self, owner: Owner) -> (Option<TimerId>, Option<TimerId>) {
        match owner {
            Owner::Engine => (self.engine.head, self.engine.tail),
            Owner::Timer(t) => (self.timers[t.0].head, self.timers[t.0].tail),
        }
    }

    fn set_owner_head(&mut self, owner: Owner, v: Option<TimerId>) {
        match owner {
            Owner::Engine => self.engine.head = v,
            Owner::Timer(t) => self.timers[t.0].head = v,
        }
    }

    fn set_owner_tail(&mut self, owner: Owner, v: Option<TimerId>) {
        match owner {
            Owner::Engine => self.engine.tail = v,
            Owner::Timer(t) => self.timers[t.0].tail = v,
        }
    }

    /// `addChild(parent, child, sortByPriority)` for timers.
    pub(crate) fn add_timer_child(&mut self, owner: Owner, child: TimerId, by_priority: bool) {
        let mut prev = self.owner_ends(owner).1;
        while let Some(p) = prev {
            if by_priority && self.timers[p.0].priority > self.timers[child.0].priority {
                prev = self.timers[p.0].prev;
            } else {
                break;
            }
        }
        let next = match prev {
            Some(p) => self.timers[p.0].next,
            None => self.owner_ends(owner).0,
        };
        match prev {
            Some(p) => self.timers[p.0].next = Some(child),
            None => self.set_owner_head(owner, Some(child)),
        }
        match next {
            Some(n) => self.timers[n.0].prev = Some(child),
            None => self.set_owner_tail(owner, Some(child)),
        }
        self.timers[child.0].prev = prev;
        self.timers[child.0].next = next;
    }

    fn remove_timer_child(&mut self, owner: Owner, child: TimerId) {
        let prev = self.timers[child.0].prev;
        let next = self.timers[child.0].next;
        match prev {
            Some(p) => self.timers[p.0].next = next,
            None => self.set_owner_head(owner, next),
        }
        match next {
            Some(n) => self.timers[n.0].prev = prev,
            None => self.set_owner_tail(owner, prev),
        }
        self.timers[child.0].prev = None;
        self.timers[child.0].next = None;
    }

    /// The children of a timeline, first to last.
    pub(crate) fn children(&self, timer: TimerId) -> Vec<TimerId> {
        let mut out = Vec::new();
        let mut next = self.timers[timer.0].head;
        while let Some(c) = next {
            out.push(c);
            next = self.timers[c.0].next;
        }
        out
    }

    /// The tweens of an animation, first to last.
    pub(crate) fn tweens_of(&self, timer: TimerId) -> Vec<TweenId> {
        let mut out = Vec::new();
        let mut next = self.timers[timer.0].tween_head;
        while let Some(t) = next {
            out.push(t);
            next = self.tweens[t.0].next;
        }
        out
    }

    pub(crate) fn add_tween(&mut self, animation: TimerId, tween: TweenId) {
        let prev = self.timers[animation.0].tween_tail;
        match prev {
            Some(p) => self.tweens[p.0].next = Some(tween),
            None => self.timers[animation.0].tween_head = Some(tween),
        }
        self.timers[animation.0].tween_tail = Some(tween);
        self.tweens[tween.0].prev = prev;
        self.tweens[tween.0].next = None;
    }

    // ---- Engine (`engine/engine.js`) ----

    /// `engine.wake()`.
    pub(crate) fn wake(&mut self) {
        if !self.engine.req_id {
            let now = self.now;
            self.engine.clock.request_tick(now);
            self.engine.req_id = true;
            self.raf_callbacks += 1;
        }
    }

    /// `engine.requestTick(now())`, which the timer constructor calls
    /// when the engine is not running.
    pub(crate) fn engine_request_tick(&mut self) {
        let now = self.now;
        self.engine.clock.request_tick(now);
    }

    /// `tickEngine`.
    fn tick_engine(&mut self) {
        if self.engine.head.is_some() {
            self.raf_callbacks += 1;
            self.engine_update();
        } else {
            self.engine.req_id = false;
        }
    }

    /// `engine.update()`.
    fn engine_update(&mut self) {
        let time = self.now;
        self.engine.clock.current_time = time;
        if !self.engine.clock.request_tick(time) {
            return;
        }
        let engine_speed = self.engine.clock.speed;
        let mut active = self.engine.head;
        while let Some(t) = active {
            let next = self.timers[t.0].next;
            if !self.timers[t.0].paused {
                let timer = &self.timers[t.0];
                let local = (time - timer.clock.start_time) * timer.clock.speed * engine_speed;
                self.tick(t, local, false, false, TickMode::Auto);
            } else {
                self.remove_timer_child(Owner::Engine, t);
                self.engine.has_children = self.engine.tail.is_some();
                self.timers[t.0].running = false;
                if self.timers[t.0].completed && !self.timers[t.0].cancelled {
                    self.cancel(t);
                }
            }
            active = next;
        }
    }

    // ---- Timer (`timer/timer.js`) ----

    /// `new Timer(parameters, parent, parentPosition)`.
    pub(crate) fn new_timer(
        &mut self,
        params: &Obj,
        parent: Option<TimerId>,
        parent_position: f64,
        timeline: bool,
    ) -> TimerId {
        self.timer_id += 1.0;
        let init_time = if parent.is_some() {
            0.0
        } else {
            self.engine.clock.last_tick_time
        };
        let defaults = match parent {
            Some(p) => self.timers[p.0].defaults.clone(),
            None => global_defaults(),
        };
        let delay = match params.get("delay") {
            None | Some(Value::Stagger(_)) | Some(Value::Ease(_)) => {
                to_number_value(defaults.get("delay").unwrap())
            }
            Some(v) => to_number_value(v),
        };
        let duration = match params.get("duration") {
            None | Some(Value::Stagger(_)) | Some(Value::Ease(_)) => f64::INFINITY,
            Some(v) => to_number_value(v),
        };
        let loop_value = set_value(params.get("loop"), defaults.get("loop").unwrap());
        let loop_delay = to_number_value(set_value(
            params.get("loopDelay"),
            defaults.get("loopDelay").unwrap(),
        ));
        let iteration_count = match loop_value {
            Value::Bool(true) => f64::INFINITY,
            v => {
                let n = to_number_value(v);
                if n == f64::INFINITY || n < 0.0 {
                    f64::INFINITY
                } else {
                    n + 1.0
                }
            }
        };
        let offset = if parent.is_some() {
            parent_position
        } else {
            if !self.engine.req_id {
                self.engine_request_tick();
            }
            self.engine.clock.last_tick_time - self.engine.clock.start_time
        };
        let total =
            super::value::clamp_infinity((duration + loop_delay) * iteration_count - loop_delay);
        let reversed = truthy(set_value(
            params.get("reversed"),
            defaults.get("reversed").unwrap(),
        )) as i32;
        // `super(0)`, then the timer's start times.
        let mut clock = Clock::new(init_time);
        clock.current_time = 0.0;
        clock.fps = to_number_value(set_value(
            params.get("frameRate"),
            defaults.get("frameRate").unwrap(),
        ));
        clock.speed = to_number_value(set_value(
            params.get("playbackRate"),
            defaults.get("playbackRate").unwrap(),
        ));
        let id = TimerId(self.timers.len());
        self.timers.push(Timer {
            labels: HashMap::new(),
            tl_composition: false,
            id: self.timer_id,
            parent,
            duration: or_min_value(total),
            backwards: false,
            paused: true,
            began: false,
            completed: false,
            iteration_duration: duration,
            iteration_count,
            autoplay: parent.is_none()
                && truthy(set_value(
                    params.get("autoplay"),
                    defaults.get("autoplay").unwrap(),
                )),
            offset,
            delay,
            loop_delay,
            iteration_time: 0.0,
            current_iteration: 0.0,
            running: false,
            reversed,
            reverse: reversed,
            cancelled: false,
            alternate: truthy(set_value(
                params.get("alternate"),
                defaults.get("alternate").unwrap(),
            )),
            prev: None,
            next: None,
            head: None,
            tail: None,
            tween_head: None,
            tween_tail: None,
            has_children: false,
            clock,
            priority: 1.0,
            ease: None,
            defaults: if timeline {
                global_defaults()
            } else {
                Obj::default()
            },
            targets: Vec::new(),
        });
        id
    }

    /// `timer.init(internalRender)`.
    pub(crate) fn init(&mut self, id: TimerId, internal: bool) {
        // `this.fps = this._fps`
        let fps = self.timers[id.0].clock.fps;
        let fps = if fps < MIN_VALUE { MIN_VALUE } else { fps };
        self.timers[id.0].clock.fps = fps;
        self.timers[id.0].clock.frame_duration = 1000.0 / fps;
        // `this.speed = this._speed`
        let speed = self.timers[id.0].clock.speed;
        self.timers[id.0].clock.speed = if speed < MIN_VALUE { MIN_VALUE } else { speed };
        self.reset_time(id);
        if !internal && self.timers[id.0].has_children {
            let duration = self.timers[id.0].duration;
            self.tick(id, duration, true, internal, TickMode::Force);
        }
        self.reset(id, internal);
        if self.timers[id.0].autoplay {
            self.resume(id);
        }
    }

    /// `timer.reset(softReset)`.
    fn reset(&mut self, id: TimerId, soft: bool) {
        self.revive(id);
        debug_assert!(
            !(self.timers[id.0].reversed != 0 && self.timers[id.0].reverse == 0),
            "alternate() and reverse() are not ported"
        );
        self.timers[id.0].iteration_time = self.timers[id.0].iteration_duration;
        self.tick(id, 0.0, true, soft, TickMode::Force);
        reset_timer_properties(&mut self.timers[id.0]);
        if self.timers[id.0].has_children {
            for child in self.children(id) {
                reset_timer_properties(&mut self.timers[child.0]);
            }
        }
    }

    /// `reviveTimer(timer)`.
    fn revive(&mut self, id: TimerId) {
        if !self.timers[id.0].cancelled {
            return;
        }
        if self.timers[id.0].has_children {
            for child in self.children(id) {
                self.revive(child);
            }
        } else {
            for tween in self.tweens_of(id) {
                if self.tweens[tween.0].composition != Composition::None {
                    self.compose_tween(tween);
                }
            }
        }
        self.timers[id.0].cancelled = false;
    }

    /// `timer.resetTime()`.
    fn reset_time(&mut self, id: TimerId) {
        let engine_speed = self.engine.clock.speed;
        let now = self.now;
        let timer = &mut self.timers[id.0];
        let time_scale = 1.0 / (timer.clock.speed * engine_speed);
        timer.clock.start_time = now - (timer.clock.current_time + timer.delay) * time_scale;
    }

    /// `timer.pause()`.
    fn pause(&mut self, id: TimerId) {
        self.timers[id.0].paused = true;
    }

    /// `timer.resume()`.
    pub(crate) fn resume(&mut self, id: TimerId) {
        if !self.timers[id.0].paused {
            return;
        }
        self.timers[id.0].paused = false;
        if self.timers[id.0].duration <= MIN_VALUE && !self.timers[id.0].has_children {
            self.tick(id, MIN_VALUE, false, false, TickMode::Force);
        } else {
            if !self.timers[id.0].running {
                self.add_timer_child(Owner::Engine, id, true);
                self.engine.has_children = true;
                self.timers[id.0].running = true;
            }
            self.reset_time(id);
            self.timers[id.0].clock.start_time -= 12.0;
            self.wake();
        }
    }

    /// `timer.cancel()`.
    pub(crate) fn cancel(&mut self, id: TimerId) {
        if self.timers[id.0].has_children {
            for child in self.children(id).into_iter().rev() {
                self.cancel(child);
            }
        } else {
            for tween in self.tweens_of(id) {
                self.remove_tween_siblings(tween);
            }
        }
        self.timers[id.0].cancelled = true;
        self.pause(id);
    }

    // ---- Rendering (`core/render.js`) ----

    /// `render(tickable, time, muteCallbacks, internalRender, tickMode)`.
    /// Callbacks are not ported; `muted` only selects the branches that
    /// update `completed`.
    fn render(
        &mut self,
        id: TimerId,
        time: f64,
        muted: bool,
        internal: bool,
        mode: TickMode,
    ) -> bool {
        let t = &self.timers[id.0];
        let parent = t.parent;
        let duration = t.duration;
        let completed = t.completed;
        let iteration_duration = t.iteration_duration;
        let iteration_count = t.iteration_count;
        let loop_delay = t.loop_delay;
        let reversed = t.reversed;
        let alternate = t.alternate;
        let has_children = t.has_children;
        let delay = t.delay;
        let prev_absolute_time = t.clock.current_time;
        let end_time = delay + iteration_duration;
        let absolute_time = time - delay;
        let prev_time = clamp(prev_absolute_time, -delay, duration);
        let current_time = clamp(absolute_time, -delay, duration);
        let delta_time = absolute_time - prev_absolute_time;
        let above_zero = current_time > 0.0;
        let at_end = current_time >= duration;
        let is_setter = duration <= MIN_VALUE;
        let forced = mode == TickMode::Force;
        let mut is_odd = 0.0;
        let mut elapsed = absolute_time;
        let mut has_rendered = false;
        if iteration_count > 1.0 {
            let period = iteration_duration + if at_end { 0.0 } else { loop_delay };
            let iteration = to_int32(current_time / period);
            let mut current = clamp(iteration, 0.0, iteration_count);
            if at_end {
                current -= 1.0;
            }
            self.timers[id.0].current_iteration = current;
            is_odd = current % 2.0;
            let e = current_time - iteration * period;
            elapsed = if e == 0.0 || e.is_nan() { 0.0 } else { e };
        }
        let is_reversed = (reversed
            ^ if alternate {
                to_int32(is_odd) as i32
            } else {
                0
            })
            != 0;
        let t = &self.timers[id.0];
        let mut iteration_time = if at_end {
            if is_reversed {
                0.0
            } else {
                duration
            }
        } else if is_reversed {
            iteration_duration - elapsed
        } else {
            elapsed
        };
        if let Some(ease) = &t.ease {
            let v = iteration_duration * ease(iteration_time / iteration_duration);
            iteration_time = if v == 0.0 || v.is_nan() { 0.0 } else { v };
        }
        let direction = match parent {
            Some(p) => self.timers[p.0].backwards,
            None => absolute_time < prev_absolute_time,
        };
        let running_backwards = if direction { !is_reversed } else { is_reversed };
        let t = &mut self.timers[id.0];
        t.clock.current_time = absolute_time;
        t.iteration_time = iteration_time;
        t.backwards = running_backwards;
        if above_zero && !t.began {
            t.began = true;
        } else if absolute_time <= 0.0 {
            t.began = false;
        }
        let gate = forced
            || mode == TickMode::Auto
                && (time
                    >= (if parent.is_some() && delay > 0.0 {
                        0.0
                    } else {
                        delay
                    })
                    && time <= end_time
                    || time <= delay && prev_time > delay
                    || time >= end_time && prev_time != duration)
            || iteration_time >= end_time && prev_time != duration
            || iteration_time <= delay && prev_time > 0.0 && !at_end
            || time <= prev_time && prev_time == duration && completed
            || at_end && !completed && is_setter;
        if gate && !has_children {
            let forced_render = forced
                || (if running_backwards {
                    -delta_time
                } else {
                    delta_time
                }) >= TICK_THRESHOLD;
            let parent_offset = parent.map_or(0.0, |p| self.timers[p.0].offset);
            let abs_time = round(
                self.timers[id.0].offset + parent_offset + delay + iteration_time,
                12,
            );
            has_rendered = self.render_tweens(
                id,
                abs_time,
                iteration_time,
                forced_render,
                internal,
                above_zero,
            );
        }
        let t = &mut self.timers[id.0];
        if let Some(parent) = parent.filter(|_| is_setter) {
            if !muted {
                let parent_began = self.timers[parent.0].began;
                let t = &mut self.timers[id.0];
                if parent_began && !running_backwards && absolute_time > 0.0 && !completed
                    || running_backwards && absolute_time <= MIN_VALUE && completed
                {
                    t.completed = !running_backwards;
                }
            }
        } else if above_zero && at_end {
            if iteration_count == f64::INFINITY {
                t.clock.start_time += t.duration;
            } else if t.current_iteration >= iteration_count - 1.0 {
                t.paused = true;
                if !completed && !has_children {
                    t.completed = true;
                }
            }
        } else {
            t.completed = false;
        }
        has_rendered
    }

    /// The tween loop of `render()`.
    fn render_tweens(
        &mut self,
        id: TimerId,
        absolute_time: f64,
        iteration_time: f64,
        forced_render: bool,
        internal: bool,
        above_zero: bool,
    ) -> bool {
        let mut has_rendered = false;
        let mut transforms_target: Option<TargetId> = None;
        let mut style_target: Option<TargetId> = None;
        let mut transforms_need_update = false;
        let mut next = self.timers[id.0].tween_head;
        while let Some(tid) = next {
            let tw = &self.tweens[tid.0];
            let current = tw.current_time;
            let change = tw.change_duration;
            let abs_end = tw.absolute_start_time + tw.change_duration;
            let has_composition = tw.composition != Composition::None;
            let prev_rep = tw.prev_rep.map(|p| &self.tweens[p.0]);
            let next_rep = tw.next_rep.map(|n| &self.tweens[n.0]);
            let prev_rep_end = prev_rep.map_or(0.0, |p| p.absolute_start_time + p.change_duration);
            let prev_cross = prev_rep.is_some_and(|p| p.parent != tw.parent);
            let next_takeover = match next_rep {
                None => abs_end,
                Some(n) if n.is_overridden => abs_end,
                Some(n) if n.parent == tw.parent => abs_end + n.delay,
                Some(n) => {
                    if n.absolute_start_time < n.absolute_update_start_time {
                        n.absolute_start_time
                    } else {
                        n.absolute_update_start_time
                    }
                }
            };
            let renders = (forced_render
                || (current != change
                    || absolute_time <= next_takeover
                    || (prev_rep.is_some()
                        && !prev_cross
                        && next_rep.is_none_or(|n| n.parent != tw.parent)))
                    && (current != 0.0
                        || absolute_time >= tw.absolute_start_time
                        || (prev_cross
                            && !tw.has_from_value
                            && !prev_rep.unwrap().is_overridden
                            && absolute_time >= prev_rep_end)
                        || next_rep.is_some_and(|n| {
                            !n.is_overridden
                                && n.parent == tw.parent
                                && n.current_time != 0.0
                                && iteration_time < n.start_time
                        })))
                && (prev_rep.is_none() || prev_cross || iteration_time >= tw.start_time)
                && (!has_composition
                    || (!tw.is_overridden
                        && (!tw.is_overlapped || absolute_time <= abs_end)
                        && next_rep
                            .is_none_or(|n| n.is_overridden || absolute_time <= next_takeover)
                        && prev_rep.is_none_or(|p| {
                            p.is_overridden
                                || if !prev_cross {
                                    absolute_time >= prev_rep_end + tw.delay
                                } else {
                                    absolute_time >= tw.absolute_start_time
                                        || (!tw.has_from_value && absolute_time >= prev_rep_end)
                                }
                        })));
            if renders {
                let tw = &mut self.tweens[tid.0];
                let new_time = clamp(iteration_time - tw.start_time, 0.0, change);
                tw.current_time = new_time;
                let progress = (tw.ease)(new_time / tw.update_duration);
                let precision = if progress == 0.0 || progress == 1.0 {
                    -1
                } else {
                    PRECISION
                };
                let number = round(lerp(tw.from_number, tw.to_number, progress), precision);
                let value = match tw.value_type {
                    ValueType::Number => Raw::Number(number),
                    ValueType::Unit => Raw::String(format!(
                        "{}{}",
                        number_to_string(number),
                        tw.unit.as_deref().unwrap_or("")
                    )),
                };
                if has_composition {
                    tw.number = number;
                }
                if !internal {
                    let target = tw.target;
                    let property = tw.property.clone();
                    let tween_type = tw.tween_type;
                    let text = super::value::raw_to_string(&value);
                    style_target = Some(target);
                    match tween_type {
                        TweenType::Transform => {
                            if Some(target) != transforms_target {
                                transforms_target = Some(target);
                            }
                            self.target_mut(target).transforms.insert(property, text);
                            transforms_need_update = true;
                        }
                        TweenType::Css => self.write_style(target, &property, text),
                    }
                    if above_zero {
                        has_rendered = true;
                    }
                } else {
                    tw.value = value;
                }
            } else if current != 0.0
                && !current.is_nan()
                && prev_rep.is_some()
                && !prev_cross
                && iteration_time < tw.start_time
            {
                self.tweens[tid.0].current_time = 0.0;
            }
            if transforms_need_update && self.tweens[tid.0].render_transforms {
                let cache = &self.targets[transforms_target.unwrap().0].transforms;
                let text = build_transform_string(cache);
                self.write_style(style_target.unwrap(), "transform", text);
                transforms_need_update = false;
            }
            next = self.tweens[tid.0].next;
        }
        has_rendered
    }

    /// `tick(tickable, time, muteCallbacks, internalRender, tickMode)`.
    pub(crate) fn tick(
        &mut self,
        id: TimerId,
        time: f64,
        muted: bool,
        internal: bool,
        mode: TickMode,
    ) {
        let current_iteration = self.timers[id.0].current_iteration;
        self.render(id, time, muted, internal, mode);
        if !self.timers[id.0].has_children {
            return;
        }
        let backwards = self.timers[id.0].backwards;
        let children_time = if internal {
            time
        } else {
            self.timers[id.0].iteration_time
        };
        let mut children_completed = true;
        if !internal && self.timers[id.0].current_iteration != current_iteration {
            let iteration_duration = self.timers[id.0].iteration_duration;
            for child in self.children(id) {
                if !backwards {
                    let c = &self.timers[child.0];
                    if !c.completed && !c.backwards && c.clock.current_time < c.iteration_duration {
                        self.render(child, iteration_duration, muted, true, TickMode::Force);
                    }
                    let c = &mut self.timers[child.0];
                    c.began = false;
                    c.completed = false;
                }
            }
        }
        let mut children = self.children(id);
        if backwards {
            children.reverse();
        }
        for child in children {
            let c = &self.timers[child.0];
            let child_time = round((children_time - c.offset) * c.clock.speed, 12);
            if backwards && child_time > c.delay + c.duration {
                continue;
            }
            self.render(child, child_time, muted, internal, mode);
            if !self.timers[child.0].completed && children_completed {
                children_completed = false;
            }
        }
        let t = &mut self.timers[id.0];
        if (children_completed || backwards) && t.clock.current_time >= t.duration {
            t.paused = true;
            if !t.completed {
                t.completed = true;
            }
        }
    }
}

fn reset_timer_properties(timer: &mut Timer) {
    timer.paused = true;
    timer.began = false;
    timer.completed = false;
}

/// `x || minValue` for a duration.
pub(crate) fn or_min_value(x: f64) -> f64 {
    if x == 0.0 || x.is_nan() {
        MIN_VALUE
    } else {
        x
    }
}

/// `globals.defaults`, the values the port reads.
pub(crate) fn global_defaults() -> Obj {
    Obj(vec![
        ("id".into(), Value::Null),
        ("keyframes".into(), Value::Null),
        ("playbackEase".into(), Value::Null),
        ("playbackRate".into(), Value::Number(1.0)),
        ("frameRate".into(), Value::Number(MAX_FPS)),
        ("loop".into(), Value::Number(0.0)),
        ("reversed".into(), Value::Bool(false)),
        ("alternate".into(), Value::Bool(false)),
        ("autoplay".into(), Value::Bool(true)),
        ("persist".into(), Value::Bool(false)),
        ("duration".into(), Value::Number(1000.0)),
        ("delay".into(), Value::Number(0.0)),
        ("loopDelay".into(), Value::Number(0.0)),
        ("ease".into(), Value::String("out(2)".into())),
        ("composition".into(), Value::Number(0.0)),
    ])
}

/// `setValue(targetValue, defaultValue)`.
pub(crate) fn set_value<'a>(value: Option<&'a Value>, default: &'a Value) -> &'a Value {
    value.unwrap_or(default)
}

/// JavaScript truthiness.
pub(crate) fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => *n != 0.0 && !n.is_nan(),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// JavaScript `+value`.
pub(crate) fn to_number_value(value: &Value) -> f64 {
    match value {
        Value::Null => 0.0,
        Value::Bool(b) => *b as i32 as f64,
        Value::Number(n) => *n,
        Value::String(s) => super::value::string_to_number(s),
        _ => f64::NAN,
    }
}
