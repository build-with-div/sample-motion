//! `Timeline` (`timeline/timeline.js`) and `parseTimelinePosition`
//! (`timeline/position.js`).

use super::animation::merge_objects;
use super::ease::parse_ease;
use super::engine::{
    global_defaults, or_min_value, set_value, truthy, Owner, Runtime, TickMode, TimerId,
};
use super::stagger::PositionResolver;
use super::value::{
    clamp_infinity, relative_value, string_to_number, Obj, Raw, Unsupported, Value, MIN_VALUE,
};
use super::TargetId;

impl Runtime {
    /// `createTimeline(parameters)`.
    pub fn create_timeline(&mut self, parameters: &Obj) -> Result<TimerId, Unsupported> {
        let id = self.new_timer(parameters, None, 0.0, true);
        self.tl_id += 1.0;
        let defaults = match parameters.get("defaults") {
            Some(Value::Object(d)) => merge_objects(d, &global_defaults()),
            Some(other) => return Err(Unsupported(format!("timeline defaults {other:?}"))),
            None => global_defaults(),
        };
        let composition = truthy(set_value(parameters.get("composition"), &Value::Bool(true)));
        let playback_ease = match set_value(parameters.get("playbackEase"), &Value::Null) {
            Value::Null => None,
            Value::String(s) => Some(parse_ease(&super::ease::EaseSpec::Name(s.clone()))?),
            Value::Ease(spec) => Some(parse_ease(spec)?),
            other => return Err(Unsupported(format!("playbackEase {other:?}"))),
        };
        let timer = &mut self.timers[id.0];
        timer.id = self.tl_id;
        timer.duration = 0.0;
        timer.labels.clear();
        timer.defaults = defaults;
        timer.tl_composition = composition;
        timer.ease = playback_ease;
        timer.iteration_duration = 0.0;
        self.init(id, false);
        Ok(id)
    }

    /// `timeline.add(targets, parameters, position)` for an animation;
    /// `position` may be a stagger.
    pub fn timeline_add(
        &mut self,
        tl: TimerId,
        targets: &[TargetId],
        parameters: &Obj,
        position: Option<&Value>,
    ) -> Result<(), Unsupported> {
        self.timers[tl.0].has_children = true;
        match position {
            Some(Value::Stagger(stagger)) => {
                let mut parsed: Vec<TargetId> = Vec::new();
                for target in targets {
                    if !parsed.contains(target) {
                        parsed.push(*target);
                    }
                }
                let duration = self.timers[tl.0].duration;
                let iteration_duration = self.timers[tl.0].iteration_duration;
                for (i, target) in parsed.iter().enumerate() {
                    self.timers[tl.0].duration = duration;
                    self.timers[tl.0].iteration_duration = iteration_duration;
                    let raw = stagger.call(i, parsed.len(), Some(&TimelineRef { rt: self, tl }))?;
                    let position = self.parse_position(tl, &raw)?;
                    self.add_tl_child(parameters, tl, position, &[*target], i, Some(&parsed))?;
                }
            }
            other => {
                let raw = position_raw(other)?;
                let position = self.parse_position(tl, &raw)?;
                self.add_tl_child(parameters, tl, position, targets, 0, None)?;
            }
        }
        if self.timers[tl.0].tl_composition {
            self.init(tl, true);
        }
        Ok(())
    }

    /// `timeline.set(targets, parameters, position)`.
    pub fn timeline_set(
        &mut self,
        tl: TimerId,
        targets: &[TargetId],
        parameters: &Obj,
        position: Option<&Value>,
    ) -> Result<(), Unsupported> {
        let mut parameters = parameters.clone();
        parameters.set("duration", Value::Number(MIN_VALUE));
        parameters.set("composition", Value::Number(0.0));
        self.timeline_add(tl, targets, &parameters, position)
    }

    /// `timeline.label(labelName, position)`.
    pub fn timeline_label(
        &mut self,
        tl: TimerId,
        name: &str,
        position: Option<&Value>,
    ) -> Result<(), Unsupported> {
        let raw = position_raw(position)?;
        let position = self.parse_position(tl, &raw)?;
        self.timers[tl.0].labels.insert(name.to_string(), position);
        Ok(())
    }

    /// `addTlChild(childParams, tl, timePosition, targets, index,
    /// allTargets)`.
    fn add_tl_child(
        &mut self,
        parameters: &Obj,
        tl: TimerId,
        position: f64,
        targets: &[TargetId],
        index: usize,
        all_targets: Option<&[TargetId]>,
    ) -> Result<(), Unsupported> {
        let is_setter =
            matches!(parameters.get("duration"), Some(Value::Number(d)) if *d <= MIN_VALUE);
        let adjusted = if is_setter {
            position - MIN_VALUE
        } else {
            position
        };
        let composition = self.timers[tl.0].tl_composition;
        if composition {
            self.tick(tl, adjusted, true, true, TickMode::Auto);
        }
        let child =
            self.new_animation(targets, parameters, Some(tl), adjusted, index, all_targets)?;
        if composition {
            self.init(child, true);
        }
        self.add_timer_child(Owner::Timer(tl), child, false);
        for c in self.children(tl) {
            let c = &self.timers[c.0];
            let end = c.offset + c.delay + c.duration;
            if end > self.timers[tl.0].iteration_duration {
                self.timers[tl.0].iteration_duration = end;
            }
        }
        let t = &mut self.timers[tl.0];
        t.duration = or_min_value(clamp_infinity(
            (t.iteration_duration + t.loop_delay) * t.iteration_count - t.loop_delay,
        ));
        Ok(())
    }

    /// `parseTimelinePosition(timeline, timePosition)`.
    pub(crate) fn parse_position(&self, tl: TimerId, position: &Raw) -> Result<f64, Unsupported> {
        let timer = &self.timers[tl.0];
        let mut tl_duration = timer.iteration_duration;
        if tl_duration == MIN_VALUE {
            tl_duration = 0.0;
        }
        let text = match position {
            Raw::Undefined => return Ok(tl_duration),
            Raw::Number(n) => return Ok(*n),
            Raw::String(s) => {
                let n = string_to_number(s);
                if !n.is_nan() {
                    return Ok(n);
                }
                s.as_str()
            }
        };
        let labels = &timer.labels;
        // `getPrevChildOffset`
        let prev_offset = if let Some(rest) = text.strip_prefix('<') {
            let prev = timer.tail.map(|p| &self.timers[p.0]);
            let offset = prev.map_or(0.0, |p| p.offset + p.delay);
            Some(if rest.starts_with('<') {
                offset
            } else {
                match prev {
                    Some(p) => offset + p.duration,
                    None => {
                        return Err(Unsupported(
                            "'<' with no previous child: anime.js throws".into(),
                        ))
                    }
                }
            })
        } else {
            None
        };
        // `/(\*=|\+=|-=)/`: the first operator.
        let operator = ["*=", "+=", "-="]
            .iter()
            .filter_map(|op| text.find(op).map(|i| (i, *op)))
            .min_by_key(|(i, _)| *i);
        let label = |name: &str| labels.get(name).copied();
        if let Some((_, op)) = operator {
            let mut split = text.split(op);
            let name = split.next().unwrap_or("");
            let amount = string_to_number(split.next().unwrap_or(""));
            let label_offset = if !name.is_empty() {
                label(name).unwrap_or(f64::NAN)
            } else {
                tl_duration
            };
            let base = prev_offset.unwrap_or(label_offset);
            Ok(relative_value(base, amount, op.chars().next().unwrap()))
        } else {
            Ok(match prev_offset {
                Some(o) => o,
                None => label(text).unwrap_or(tl_duration),
            })
        }
    }
}

/// A timeline position parameter as a value.
fn position_raw(position: Option<&Value>) -> Result<Raw, Unsupported> {
    match position {
        None => Ok(Raw::Undefined),
        Some(Value::Number(n)) => Ok(Raw::Number(*n)),
        Some(Value::String(s)) => Ok(Raw::String(s.clone())),
        Some(other) => Err(Unsupported(format!("the timeline position {other:?}"))),
    }
}

struct TimelineRef<'a> {
    rt: &'a Runtime,
    tl: TimerId,
}

impl PositionResolver for TimelineRef<'_> {
    fn iteration_duration(&self) -> f64 {
        self.rt.timers[self.tl.0].iteration_duration
    }

    fn parse_position(&self, position: &Raw) -> Result<f64, Unsupported> {
        self.rt.parse_position(self.tl, position)
    }
}
