//! `SpringValue` for numbers (`@react-spring/core` 10.1.2 `SpringValue.ts`,
//! `scheduleProps.ts`, `runAsync.ts`) with its `AnimatedValue` node
//! (`@react-spring/animated` 10.1.2).
//!
//! Supported props: `to` (a number, or an array of update objects run as a
//! chain), `from`, `config`, `delay`, `immediate`, `reset` and `default`.
//! Not ported: strings, arrays, `FluidValue` goals, `loop`, `reverse`,
//! `pause`, `cancel`, async `to` functions, nested chains, `onRest` and the
//! other event props, and the `Controller`/hooks layer.

use super::config::{merge_config, AnimationConfig, ConfigProps, Decay};
use super::rafz::Task;
use super::Runtime;
use crate::motion::js::{math_exp, math_round};
use std::rc::Rc;

/// A spring in a [`Runtime`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpringId(pub(crate) usize);

/// An update: the object passed to `spring.start(props)`.
#[derive(Debug, Clone, Default)]
pub struct SpringProps {
    pub to: Option<To>,
    pub from: Option<f64>,
    /// The `config` object. Its identity matters, as in JavaScript: an
    /// update whose config is not the spring's default config object merges
    /// over the default one.
    pub config: Option<Rc<ConfigProps>>,
    /// Milliseconds.
    pub delay: Option<f64>,
    pub immediate: Option<bool>,
    pub reset: Option<bool>,
    pub default: Option<bool>,
}

/// A `to` prop.
#[derive(Debug, Clone)]
pub enum To {
    Value(f64),
    /// An array of update objects, run one after the other (`runAsync`).
    Chain(Rc<Vec<SpringProps>>),
}

/// An update the port does not support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported(pub &'static str);

impl SpringProps {
    pub(crate) fn check(&self) -> Result<(), Unsupported> {
        if let Some(To::Chain(steps)) = &self.to {
            for step in steps.iter() {
                if matches!(step.to, Some(To::Chain(_))) {
                    return Err(Unsupported("a chain inside a chain"));
                }
            }
        }
        Ok(())
    }
}

/// `AnimatedValue` holding a number.
#[derive(Debug, Clone)]
struct Node {
    value: f64,
    done: bool,
    duration_progress: f64,
    last_position: f64,
    elapsed_time: f64,
    v0: Option<f64>,
    last_velocity: Option<f64>,
}

impl Node {
    fn new(value: f64) -> Self {
        Self {
            value,
            done: true,
            duration_progress: 0.0,
            last_position: value,
            elapsed_time: 0.0,
            v0: None,
            last_velocity: None,
        }
    }

    /// `setValue(value, step)`: returns true when the value changed.
    fn set_value(&mut self, value: f64, step: Option<f64>) -> bool {
        let mut value = value;
        self.last_position = value;
        if let Some(step) = step.filter(|step| truthy(*step)) {
            value = math_round(value / step) * step;
            if self.done {
                self.last_position = value;
            }
        }
        // `this._value === value`
        if self.value == value {
            return false;
        }
        self.value = value;
        true
    }

    /// `reset()`: a node that was still moving keeps its velocity.
    fn reset(&mut self) {
        let done = self.done;
        self.done = false;
        self.elapsed_time = 0.0;
        self.duration_progress = 0.0;
        self.last_position = self.value;
        if done {
            self.last_velocity = None;
        }
        self.v0 = None;
    }
}

/// `Animation`.
#[derive(Debug, Clone, Default)]
struct Animation {
    changed: bool,
    /// `values` holds the node (true) or is still the empty array.
    has_values: bool,
    to_values: Option<f64>,
    from_values: Option<f64>,
    config: AnimationConfig,
    immediate: bool,
    to: Option<f64>,
    from: Option<f64>,
}

/// The async part of `SpringValue._state`.
#[derive(Debug, Clone, Default)]
struct AsyncState {
    delayed: bool,
    async_id: Option<u64>,
    async_to: Option<Rc<Vec<SpringProps>>>,
    /// Ids of this spring's `raf.setTimeout` entries.
    timeouts: Vec<u64>,
}

/// What a resolved update promise leads to.
#[derive(Debug, Clone)]
pub(crate) enum Resolver {
    /// Nothing awaits it.
    None,
    /// A step of a chain: the chain continues with step `next`.
    ChainStep(ChainStep),
}

#[derive(Debug, Clone)]
pub(crate) struct ChainStep {
    pub spring: SpringId,
    pub chain_call_id: u64,
    pub steps: Rc<Vec<SpringProps>>,
    pub next: usize,
    /// `getDefaultProps(props)`: the chain's `config`, which each step
    /// inherits when it has none.
    pub defaults: Option<Rc<ConfigProps>>,
}

/// `{ to, from }` from `_prepareNode`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Range {
    to: Option<f64>,
    from: Option<f64>,
}

/// An update waiting for its `delay` (scheduleProps' `onStart`).
#[derive(Debug, Clone)]
pub(crate) struct ScheduledCall {
    spring: SpringId,
    range: Range,
    props: SpringProps,
    call_id: u64,
    resolver: Resolver,
}

const HAS_ANIMATED: u8 = 1;
const IS_ANIMATING: u8 = 2;

/// `SpringValue` holding a number.
#[derive(Debug, Clone, Default)]
pub(crate) struct SpringValue {
    node: Option<Node>,
    animation: Animation,
    /// `defaultProps.config`.
    default_config: Option<Rc<ConfigProps>>,
    state: AsyncState,
    pending_calls: Vec<Resolver>,
    last_call_id: u64,
    last_to_id: u64,
    memoized_duration: f64,
    phase: u8,
}

/// JavaScript truthiness of a number.
fn truthy(value: f64) -> bool {
    value != 0.0 && !value.is_nan()
}

fn decay_truthy(decay: Option<Decay>) -> bool {
    match decay {
        None => false,
        Some(Decay::Default) => true,
        Some(Decay::Rate(rate)) => truthy(rate),
    }
}

/// `Math.max(a, b)` (NaN wins).
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

/// `Math.min(a, b)` (NaN wins).
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

impl SpringValue {
    fn has_animated(&self) -> bool {
        self.phase & HAS_ANIMATED != 0
    }

    pub(crate) fn is_animating(&self) -> bool {
        self.phase & IS_ANIMATING != 0
    }

    /// `idle`: not advancing on each frame. Pausing is not ported.
    pub(crate) fn idle(&self) -> bool {
        !(self.is_animating() || self.state.async_to.is_some())
    }

    /// `get()`.
    pub(crate) fn get(&self) -> Option<f64> {
        self.node.as_ref().map(|node| node.value)
    }

    /// The `velocity` getter: `node.lastVelocity || 0`.
    pub(crate) fn velocity(&self) -> f64 {
        match self.node.as_ref().and_then(|node| node.last_velocity) {
            Some(velocity) if truthy(velocity) => velocity,
            _ => 0.0,
        }
    }

    /// `_onChange(value, idle)`: a change outside `_set` calls `_onStart`.
    fn on_change(&mut self, idle: bool) {
        if !idle && !self.animation.changed {
            self.animation.changed = true;
        }
    }

    /// `_set(value, idle)`.
    fn set(&mut self, value: Option<f64>, idle: bool) {
        let Some(value) = value else {
            return;
        };
        match &mut self.node {
            None => self.node = Some(Node::new(value)),
            Some(node) => {
                // `!isEqual(value, oldNode.getValue())`
                if value != node.value {
                    node.set_value(value, None);
                    self.on_change(idle);
                }
            }
        }
    }

    /// `_prepareNode(props)`.
    fn prepare_node(&mut self, props: &SpringProps) -> Range {
        let to = match props.to {
            Some(To::Value(value)) => Some(value),
            _ => None,
        };
        let from = props.from;
        if !self.has_animated() {
            if from.is_some() {
                self.set(from, true);
            } else if self.node.is_none() {
                self.set(to, true);
            }
        }
        Range { to, from }
    }

    /// One frame of the animation: the `forEach` over the nodes in
    /// `advance(dt)`. Returns (idle, changed).
    fn advance_node(&mut self, dt: f64) -> (bool, bool) {
        let mut idle = true;
        let mut changed = false;
        let anim = &mut self.animation;
        if !anim.has_values {
            return (idle, changed);
        }
        let config = &anim.config;
        let node = self.node.as_mut().expect("an animated spring has a node");
        if node.done {
            return (idle, changed);
        }
        let to = anim.to_values.unwrap_or(f64::NAN);
        let mut finished = anim.immediate;
        let mut position = to;
        if !finished {
            position = node.last_position;
            if config.tension <= 0.0 {
                node.done = true;
                return (idle, changed);
            }
            node.elapsed_time += dt;
            let mut elapsed = node.elapsed_time;
            let from = anim.from_values.unwrap_or(f64::NAN);
            let v0 = *node.v0.get_or_insert(config.velocity);
            let velocity;
            let precision = match config.precision {
                Some(precision) if truthy(precision) => precision,
                _ => {
                    if from == to {
                        0.005
                    } else {
                        js_max(
                            js_max(js_max(to.abs(), from.abs()), 1.0) * f64::EPSILON,
                            js_min(1.0, (to - from).abs() * 0.001),
                        )
                    }
                }
            };
            if let Some(duration) = config.duration {
                let mut p = 1.0;
                if duration > 0.0 {
                    if self.memoized_duration != duration {
                        self.memoized_duration = duration;
                        if node.duration_progress > 0.0 {
                            node.elapsed_time = duration * node.duration_progress;
                            node.elapsed_time += dt;
                            elapsed = node.elapsed_time;
                        }
                    }
                    p = config.progress.unwrap_or(0.0) + elapsed / self.memoized_duration;
                    // `p > 1 ? 1 : p < 0 ? 0 : p` (NaN and -0 pass through)
                    p = p.clamp(0.0, 1.0);
                    node.duration_progress = p;
                }
                position = from + config.easing.apply(p) * (to - from);
                velocity = (position - node.last_position) / dt;
                finished = p == 1.0;
            } else if decay_truthy(config.decay) {
                let decay = match config.decay {
                    Some(Decay::Rate(rate)) => rate,
                    _ => 0.998,
                };
                let e = math_exp(-(1.0 - decay) * elapsed);
                position = from + v0 / (1.0 - decay) * (1.0 - e);
                finished = (node.last_position - position).abs() <= precision;
                velocity = v0 * e;
            } else {
                let mut v = node.last_velocity.unwrap_or(v0);
                let rest_velocity = match config.rest_velocity {
                    Some(rest) if truthy(rest) => rest,
                    _ => precision / 10.0,
                };
                let bounce_factor = if config.clamp {
                    Some(0.0)
                } else {
                    config.bounce
                };
                let is_growing = if from == to { v0 > 0.0 } else { from < to };
                let step = 1.0;
                let num_steps = (dt / step).ceil();
                let mut n = 0.0;
                while n < num_steps {
                    let is_moving = v.abs() > rest_velocity;
                    if !is_moving {
                        finished = (to - position).abs() <= precision;
                        if finished {
                            break;
                        }
                    }
                    if let Some(bounce_factor) = bounce_factor {
                        let is_bouncing = position == to || (position > to) == is_growing;
                        if is_bouncing {
                            v = -v * bounce_factor;
                            position = to;
                        }
                    }
                    let acceleration = (-config.tension * 1e-6 * (position - to)
                        + -config.friction * 0.001 * v)
                        / config.mass;
                    v += acceleration * step;
                    position += v * step;
                    n += 1.0;
                }
                velocity = v;
            }
            node.last_velocity = Some(velocity);
            if position.is_nan() {
                finished = true;
            }
        }
        if finished {
            node.done = true;
        } else {
            idle = false;
        }
        if node.set_value(position, config.round) {
            changed = true;
        }
        (idle, changed)
    }
}

impl Runtime {
    fn spring(&mut self, id: SpringId) -> &mut SpringValue {
        &mut self.springs[id.0]
    }

    /// `new SpringValue(props)`: `start(props)` with `default: true` unless
    /// the props say otherwise.
    pub(crate) fn create_spring(&mut self, mut props: SpringProps) -> SpringId {
        let id = SpringId(self.springs.len());
        self.springs.push(SpringValue::default());
        if props.default.is_none() {
            props.default = Some(true);
        }
        self.update_spring(id, props, Resolver::None);
        id
    }

    /// `_update(props)` and `scheduleProps`.
    pub(crate) fn update_spring(&mut self, id: SpringId, props: SpringProps, resolver: Resolver) {
        let now = self.now;
        let spring = self.spring(id);
        if props.default == Some(true) {
            if let Some(config) = &props.config {
                spring.default_config = Some(Rc::clone(config));
            }
        }
        let range = spring.prepare_node(&props);
        spring.last_call_id += 1;
        let call_id = spring.last_call_id;
        // `callProp(props.delay || 0)`
        let delay = props.delay.filter(|delay| truthy(*delay)).unwrap_or(0.0);
        let call = ScheduledCall {
            spring: id,
            range,
            props,
            call_id,
            resolver,
        };
        // Not paused: `actions.resume()` does nothing, then `onResume()`.
        if delay > 0.0 {
            self.spring(id).state.delayed = true;
            let timeout = self.raf.set_timeout(now, call, delay);
            self.spring(id).state.timeouts.push(timeout);
        } else {
            self.run_scheduled(call, None);
        }
    }

    /// scheduleProps' `onStart`: the update is merged.
    pub(crate) fn run_scheduled(&mut self, call: ScheduledCall, timeout: Option<u64>) {
        let spring = self.spring(call.spring);
        spring.state.delayed = false;
        if let Some(timeout) = timeout {
            spring.state.timeouts.retain(|id| *id != timeout);
        }
        self.merge(
            call.spring,
            call.range,
            call.props,
            call.call_id,
            call.resolver,
        );
    }

    /// Resolve an update's promise.
    fn resolve(&mut self, resolver: Resolver) {
        if let Resolver::ChainStep(step) = resolver {
            self.microtasks.push_back(step);
        }
    }

    /// `flushCalls(this._pendingCalls, result)`.
    fn flush_pending_calls(&mut self, id: SpringId) {
        let calls = std::mem::take(&mut self.spring(id).pending_calls);
        for resolver in calls {
            self.resolve(resolver);
        }
    }

    /// `_merge(range, props, resolve)`.
    fn merge(
        &mut self,
        id: SpringId,
        range: Range,
        props: SpringProps,
        call_id: u64,
        resolver: Resolver,
    ) {
        let has_to_prop = range.to.is_some();
        let has_from_prop = range.from.is_some();
        let spring = self.spring(id);
        if has_to_prop || has_from_prop {
            if call_id > spring.last_to_id {
                spring.last_to_id = call_id;
            } else {
                return self.resolve(resolver);
            }
        }
        let is_default = props.default == Some(true);
        let prev_to = spring.animation.to;
        let prev_from = spring.animation.from;
        let mut to = range.to.or(prev_to);
        let from = range.from.or(prev_from);
        if has_from_prop && !has_to_prop && (!is_default || to.is_none()) {
            to = from;
        }
        // `!isEqual(from, prevFrom)`: `===` on numbers.
        let has_from_changed = from != prev_from;
        if has_from_changed {
            spring.animation.from = from;
        }
        let has_to_changed = to != prev_to;
        if has_to_changed {
            spring.animation.to = to;
        }
        let chain = match &props.to {
            Some(To::Chain(steps)) => Some(Rc::clone(steps)),
            _ => None,
        };
        let has_async_to = chain.is_some();
        let config = &mut spring.animation.config;
        let decay = config.decay;
        let velocity = config.velocity;
        if (has_to_prop || has_from_prop) && !decay_truthy(config.decay) {
            config.velocity = 0.0;
        }
        if let Some(new_config) = props.config.as_ref().filter(|_| !has_async_to) {
            let default_config = match &spring.default_config {
                Some(default_config) if Rc::ptr_eq(default_config, new_config) => None,
                other => other.clone(),
            };
            merge_config(config, new_config, default_config.as_deref());
        }
        let (Some(_), Some(to)) = (spring.node.as_ref(), to) else {
            return self.resolve(resolver);
        };
        let reset = match props.reset {
            None => has_from_prop && !is_default,
            Some(reset) => from.is_some() && reset,
        };
        let value = if reset { from } else { spring.get() };
        let goal = to;
        let immediate = !has_async_to && props.immediate == Some(true);
        let mut started = false;
        let mut finished = false;
        let has_value_changed = reset || (!spring.has_animated() && has_from_changed);
        if has_to_changed || has_value_changed {
            finished = value == Some(goal);
            started = !finished;
        }
        let config = &spring.animation.config;
        if (spring.animation.immediate != immediate && !immediate)
            || config.decay != decay
            || config.velocity != velocity
        {
            started = true;
        }
        if finished && spring.is_animating() {
            if spring.animation.changed && !reset {
                started = true;
            } else if !started {
                self.stop_animation(id);
            }
        }
        let spring = self.spring(id);
        if !has_async_to {
            if started {
                spring.animation.has_values = true;
                spring.animation.to_values = Some(goal);
            }
            if spring.animation.immediate != immediate {
                spring.animation.immediate = immediate;
                if !immediate && !reset {
                    spring.set(prev_to, true);
                }
            }
            if started {
                self.flush_pending_calls(id);
                let spring = self.spring(id);
                spring.pending_calls.push(resolver.clone());
                if spring.animation.changed {
                    spring.animation.changed = !reset;
                }
            }
        }
        if reset {
            self.spring(id).set(value, true);
        }
        if let Some(steps) = chain {
            self.run_async(id, steps, &props, call_id);
            self.resolve(resolver);
        } else if started {
            self.start_animation(id);
        } else if self.spring(id).is_animating() && !has_to_changed {
            self.spring(id).pending_calls.push(resolver);
        } else {
            self.resolve(resolver);
        }
    }

    /// `_start()`.
    fn start_animation(&mut self, id: SpringId) {
        let spring = self.spring(id);
        let node = spring.node.as_mut().expect("a started spring has a node");
        node.reset();
        if !spring.animation.immediate {
            spring.animation.from_values = Some(node.last_position);
        }
        if !spring.is_animating() {
            spring.phase |= HAS_ANIMATED | IS_ANIMATING;
            // `_resume()`: `frameLoop.start(this)`.
            self.frame_loop_start(id);
        }
    }

    /// `_stop(goal)`: leave the frame loop and resolve the pending updates.
    pub(crate) fn stop_animation(&mut self, id: SpringId) {
        let spring = self.spring(id);
        if spring.is_animating() {
            spring.phase &= !IS_ANIMATING;
            if spring.animation.has_values {
                if let Some(node) = spring.node.as_mut() {
                    node.done = true;
                }
            }
            self.flush_pending_calls(id);
            self.spring(id).animation.changed = false;
        }
    }

    /// `stop()`: stop at the current value and cancel delayed updates and
    /// the running chain.
    pub(crate) fn stop_spring(&mut self, id: SpringId) {
        let spring = self.spring(id);
        if spring.animation.to.is_some() {
            spring.animation.to = spring.get();
        }
        // `stopAsync(state)`
        let timeouts = std::mem::take(&mut spring.state.timeouts);
        spring.state.async_id = None;
        spring.state.async_to = None;
        for timeout in timeouts {
            self.raf.cancel_timeout(timeout);
        }
        self.stop_animation(id);
    }

    /// `advance(dt)`.
    pub(crate) fn advance_spring(&mut self, id: SpringId, dt: f64) {
        let spring = self.spring(id);
        let (idle, changed) = spring.advance_node(dt);
        let current = spring.get();
        if idle {
            let final_value = spring.animation.to;
            let decay = decay_truthy(spring.animation.config.decay);
            if (current != final_value || changed) && !decay {
                if let (Some(node), Some(final_value)) = (spring.node.as_mut(), final_value) {
                    node.set_value(final_value, None);
                }
                spring.on_change(false);
            } else if changed && decay {
                spring.on_change(false);
            }
            self.stop_animation(id);
        } else if changed {
            spring.on_change(false);
        }
    }

    /// `runAsync(to, props, state, target)` for an array of updates. The
    /// first step starts now; each later one when the previous resolves.
    fn run_async(
        &mut self,
        id: SpringId,
        steps: Rc<Vec<SpringProps>>,
        props: &SpringProps,
        call_id: u64,
    ) {
        let spring = self.spring(id);
        spring.state.async_id = Some(call_id);
        spring.state.async_to = Some(Rc::clone(&steps));
        self.chain_step(ChainStep {
            spring: id,
            chain_call_id: call_id,
            steps,
            next: 0,
            defaults: props.config.clone(),
        });
    }

    /// The chain's `animate(props)` for step `next`, or its end.
    pub(crate) fn chain_step(&mut self, step: ChainStep) {
        let spring = self.spring(step.spring);
        // `bailIfEnded`: another chain or `stop()` took over.
        if spring.state.async_id != Some(step.chain_call_id) {
            return;
        }
        let Some(props) = step.steps.get(step.next) else {
            // The chain finished: `state.asyncId = parentId`.
            spring.state.async_id = None;
            spring.state.async_to = None;
            return;
        };
        let mut props = props.clone();
        if props.config.is_none() {
            props.config = step.defaults.clone();
        }
        let id = step.spring;
        let next = ChainStep {
            next: step.next + 1,
            ..step
        };
        self.update_spring(id, props, Resolver::ChainStep(next));
    }

    /// Whether `frameLoop` keeps advancing this spring.
    pub(crate) fn spring_idle(&self, id: SpringId) -> bool {
        self.springs[id.0].idle()
    }

    pub(crate) fn schedule_advance(&mut self) {
        self.raf.schedule_update(Task::Advance);
    }
}
