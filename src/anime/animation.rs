//! `JSAnimation` (`animation/animation.js`) and replace composition
//! (`animation/composition.js`).

use super::ease::{parse_ease, EaseSpec};
use super::engine::{
    global_defaults, or_min_value, set_value, to_number_value, truthy, Composition, Runtime,
    TimerId, Tween, TweenId, TweenType,
};
use super::value::{
    clamp_infinity, decompose_raw_value, is_key, parse_inline_transforms, relative_value, round,
    short_transform, string_to_number, Decomposed, Obj, Raw, Unsupported, Value, ValueType,
    MIN_VALUE, VALID_TRANSFORMS,
};
use super::TargetId;
use crate::motion::js::{number_to_string, parse_float};
use std::f64::consts::PI;

/// A property value once the keyframe is read: one value, or `[from, to]`.
enum ToValue {
    Raw(Raw),
    Pair(Value, Value),
}

impl Runtime {
    /// `getFunctionValue(value, target, index, targets)` for a scalar
    /// parameter: a stagger runs; numbers and strings pass through.
    pub(crate) fn function_value(
        &self,
        value: Option<&Value>,
        index: usize,
        total: usize,
    ) -> Result<Raw, Unsupported> {
        match value {
            None => Ok(Raw::Undefined),
            Some(Value::Stagger(stagger)) => {
                let computed = stagger.call(index, total, None)?;
                // `!isNaN(+computed) ? +computed : computed || 0`
                Ok(match computed {
                    Raw::Number(n) if n.is_nan() => Raw::Number(0.0),
                    Raw::String(s) => {
                        let n = string_to_number(&s);
                        if !n.is_nan() {
                            Raw::Number(n)
                        } else {
                            Raw::String(s)
                        }
                    }
                    other => other,
                })
            }
            Some(Value::Number(n)) => Ok(Raw::Number(*n)),
            Some(Value::String(s)) => Ok(Raw::String(s.clone())),
            Some(other) => Err(Unsupported(format!(
                "a {other:?} where anime.js expects a number or a string"
            ))),
        }
    }

    /// `getOriginalAnimatableValue(target, propName, tweenType)`.
    fn original_value(&mut self, target: TargetId, property: &str, tween_type: TweenType) -> Raw {
        match tween_type {
            TweenType::Transform => {
                let state = self.target_mut(target);
                let inline = state.inline.get("transform").cloned().unwrap_or_default();
                Raw::String(parse_inline_transforms(
                    &inline,
                    &mut state.transforms,
                    property,
                ))
            }
            TweenType::Css => {
                // `getCSSValue`: the inline style, else the computed value.
                let inline = self
                    .target_mut(target)
                    .inline
                    .get(property)
                    .cloned()
                    .unwrap_or_default();
                let value = if inline.is_empty() {
                    self.host.computed_value(target, property)
                } else {
                    inline
                };
                Raw::String(if value == "auto" { "0".into() } else { value })
            }
        }
    }

    /// `new JSAnimation(targets, parameters, parent, parentPosition, false,
    /// index, allTargets)`.
    pub(crate) fn new_animation(
        &mut self,
        targets: &[TargetId],
        parameters: &Obj,
        parent: Option<TimerId>,
        parent_position: f64,
        index: usize,
        all_targets: Option<&[TargetId]>,
    ) -> Result<TimerId, Unsupported> {
        let id = self.new_timer(parameters, parent, parent_position, false);
        self.js_animation_id += 1.0;
        // `registerTargets`: arrays are flattened without duplicates.
        let mut parsed: Vec<TargetId> = Vec::new();
        for target in targets {
            if !parsed.contains(target) {
                parsed.push(*target);
            }
        }
        for target in &parsed {
            self.target_mut(*target);
        }
        let params = match parameters.get("keyframes") {
            Some(keyframes) => {
                merge_objects(&generate_keyframes(keyframes, parameters)?, parameters)
            }
            None => parameters.clone(),
        };
        let defaults = match parent {
            Some(p) => self.timers[p.0].defaults.clone(),
            None => global_defaults(),
        };
        let anim_playback_ease = set_value(
            params.get("playbackEase"),
            defaults.get("playbackEase").unwrap(),
        );
        let parsed_playback_ease = if truthy(anim_playback_ease) {
            Some(parse_ease(&ease_spec(anim_playback_ease)?)?)
        } else {
            None
        };
        let linear = Value::String("linear".into());
        let t_easing = set_value(
            params.get("ease"),
            if parsed_playback_ease.is_some() {
                &linear
            } else {
                defaults.get("ease").unwrap()
            },
        )
        .clone();
        let t_duration =
            set_value(params.get("duration"), defaults.get("duration").unwrap()).clone();
        let t_delay = set_value(params.get("delay"), defaults.get("delay").unwrap()).clone();
        let t_composition = match params.get("composition") {
            None if parsed.len() >= 1000 => Value::String("none".into()),
            Some(c) => c.clone(),
            None => defaults.get("composition").unwrap().clone(),
        };
        let absolute_offset_time =
            self.timers[id.0].offset + parent.map_or(0.0, |p| self.timers[p.0].offset);
        let mut iteration_duration = f64::NAN;
        let mut iteration_delay = f64::NAN;
        let mut animation_length = 0usize;
        for (target_index, &target) in parsed.iter().enumerate() {
            let ti = if index != 0 { index } else { target_index };
            let total = all_targets.map_or(parsed.len(), <[TargetId]>::len);
            let mut last_transform_group: Option<(usize, usize)> = None;
            for (p, prop_value) in params.0.iter() {
                if !is_key(p) {
                    continue;
                }
                let (tween_type, prop_name) = if VALID_TRANSFORMS.contains(&p.as_str()) {
                    (TweenType::Transform, p.clone())
                } else if let Some(full) = short_transform(p) {
                    (TweenType::Transform, full.to_string())
                } else if p == "opacity" {
                    (TweenType::Css, p.clone())
                } else {
                    return Err(Unsupported(format!(
                        "the property {p:?}: the port animates opacity and transforms"
                    )));
                };
                // Normalise to keyframes: `[x, y]` is one from-to keyframe,
                // `[x, y, z]` is `[[x, y], z]`, an array of objects is kept.
                let keyframes: Vec<Value> = match prop_value {
                    Value::Array(values) if !matches!(values.first(), Some(Value::Object(_))) => {
                        if values.len() == 2 {
                            vec![Value::Object(Obj(vec![(
                                "to".into(),
                                Value::Array(values.clone()),
                            )]))]
                        } else if values.len() > 2 {
                            let mut kf =
                                vec![Value::Array(vec![values[0].clone(), values[1].clone()])];
                            kf.extend(values[2..].iter().cloned());
                            kf
                        } else {
                            values.clone()
                        }
                    }
                    Value::Array(values) => values.clone(),
                    other => vec![other.clone()],
                };
                let siblings_key = (target, prop_name.clone());
                let mut has_siblings = false;
                let mut prev_tween: Option<TweenId> = None;
                let mut first_change_start = f64::NAN;
                let mut last_change_end = 0.0;
                let l = keyframes.len();
                let mut tween_index = 0;
                while tween_index < l {
                    let key = match &keyframes[tween_index] {
                        Value::Object(obj) => obj.clone(),
                        other => Obj(vec![("to".into(), other.clone())]),
                    };
                    let composition_value = set_value(key.get("composition"), &t_composition);
                    let composition =
                        match self.function_value(Some(composition_value), ti, total)? {
                            Raw::Number(0.0) => Composition::Replace,
                            Raw::Number(1.0) => Composition::None,
                            Raw::String(s) if s == "replace" => Composition::Replace,
                            Raw::String(s) if s == "none" => Composition::None,
                            other => {
                                return Err(Unsupported(format!(
                                    "composition {other:?}: blend is not ported"
                                )))
                            }
                        };
                    if !has_siblings && composition != Composition::None {
                        has_siblings = true;
                        self.rep.entry(siblings_key.clone()).or_insert((None, None));
                    }
                    let to = match key.get("to") {
                        Some(Value::Array(pair)) => {
                            if pair.len() != 2 {
                                return Err(Unsupported("a from-to array needs two values".into()));
                            }
                            ToValue::Pair(pair[0].clone(), pair[1].clone())
                        }
                        other => ToValue::Raw(self.function_value(other, ti, total)?),
                    };
                    let from_value = self.function_value(key.get("from"), ti, total)?;
                    let ease_value = match key.get("ease") {
                        Some(e) if truthy(e) => e.clone(),
                        _ => t_easing.clone(),
                    };
                    let duration_value = match key.get("duration") {
                        Some(d) => self.function_value(Some(d), ti, total)?,
                        None if l > 1 => {
                            let d = self.function_value(Some(&t_duration), ti, total)?;
                            Raw::Number(raw_number(&d) / l as f64)
                        }
                        None => self.function_value(Some(&t_duration), ti, total)?,
                    };
                    let delay_value = match key.get("delay") {
                        Some(d) => self.function_value(Some(d), ti, total)?,
                        None if tween_index == 0 => {
                            self.function_value(Some(&t_delay), ti, total)?
                        }
                        None => Raw::Number(0.0),
                    };
                    let tween_delay = raw_number(&delay_value);
                    let has_from = !from_value.is_undefined();
                    let has_to = !matches!(to, ToValue::Raw(Raw::Undefined));
                    let is_from_to_array = matches!(to, ToValue::Pair(..));
                    let is_from_to_value = is_from_to_array || (has_from && has_to);
                    let update_start_local = if prev_tween.is_some() {
                        last_change_end
                    } else {
                        0.0
                    };
                    let start_time = if prev_tween.is_some() {
                        last_change_end + tween_delay
                    } else {
                        tween_delay
                    };
                    let absolute_start_time = round(absolute_offset_time + start_time, 12);
                    let absolute_update_start_time =
                        round(absolute_offset_time + update_start_local, 12);
                    let mut prev_sibling = prev_tween;
                    if composition != Composition::None {
                        let mut next_sibling = self.rep[&siblings_key].0;
                        while let Some(n) = next_sibling {
                            if self.tweens[n.0].absolute_start_time > absolute_start_time {
                                break;
                            }
                            if !self.tweens[n.0].is_overridden {
                                prev_sibling = Some(n);
                            }
                            next_sibling = self.tweens[n.0].next_rep;
                            if let Some(n) = next_sibling {
                                if self.tweens[n.0].absolute_start_time >= absolute_start_time {
                                    let mut over = Some(n);
                                    while let Some(o) = over {
                                        override_tween(&mut self.tweens[o.0]);
                                        over = self.tweens[o.0].next_rep;
                                    }
                                    next_sibling = None;
                                }
                            }
                        }
                    }
                    // A timeline child starts from the value its previous
                    // sibling in the same timeline left.
                    let sibling_value = |rt: &Runtime| -> Option<Raw> {
                        let p = parent?;
                        let s = prev_sibling?;
                        let sibling = &rt.tweens[s.0];
                        (rt.timers[sibling.parent.0].parent == Some(p))
                            .then(|| sibling.value.clone())
                    };
                    let mut from: Decomposed;
                    let mut to_obj: Decomposed;
                    if is_from_to_value {
                        let (f, t) = match &to {
                            ToValue::Pair(a, b) => (
                                self.function_value(Some(a), ti, total)?,
                                self.function_value(Some(b), ti, total)?,
                            ),
                            ToValue::Raw(t) => (from_value.clone(), t.clone()),
                        };
                        from = decompose_raw_value(&f)?;
                        to_obj = decompose_raw_value(&t)?;
                        let original = self.original_value(target, &prop_name, tween_type);
                        if from.t == ValueType::Number {
                            if let Some(s) = prev_sibling {
                                if self.tweens[s.0].value_type == ValueType::Unit {
                                    from.t = ValueType::Unit;
                                    from.u = self.tweens[s.0].unit.clone();
                                }
                            } else {
                                let o = decompose_raw_value(&original)?;
                                if o.t == ValueType::Unit {
                                    from.t = ValueType::Unit;
                                    from.u = o.u;
                                }
                            }
                        }
                    } else {
                        to_obj = if has_to {
                            let ToValue::Raw(t) = &to else { unreachable!() };
                            decompose_raw_value(t)?
                        } else if let Some(p) = prev_tween {
                            decompose_tween_value(&self.tweens[p.0])
                        } else {
                            let raw = match sibling_value(self) {
                                Some(v) => v,
                                None => self.original_value(target, &prop_name, tween_type),
                            };
                            decompose_raw_value(&raw)?
                        };
                        from = if has_from {
                            decompose_raw_value(&from_value)?
                        } else if let Some(p) = prev_tween {
                            decompose_tween_value(&self.tweens[p.0])
                        } else {
                            let raw = match sibling_value(self) {
                                Some(v) => v,
                                None => self.original_value(target, &prop_name, tween_type),
                            };
                            decompose_raw_value(&raw)?
                        };
                    }
                    if let Some(op) = from.o {
                        let base = match prev_sibling {
                            None => {
                                let raw = self.original_value(target, &prop_name, tween_type);
                                decompose_raw_value(&raw)?.n
                            }
                            Some(s) => self.tweens[s.0].to_number,
                        };
                        from.n = relative_value(base, from.n, op);
                    }
                    if let Some(op) = to_obj.o {
                        to_obj.n = relative_value(from.n, to_obj.n, op);
                    }
                    if from.t != to_obj.t {
                        // NUMBER and UNIT: the number takes the unit.
                        if from.t == ValueType::Unit {
                            to_obj.t = ValueType::Unit;
                            to_obj.u = from.u.clone();
                        } else {
                            from.t = ValueType::Unit;
                            from.u = to_obj.u.clone();
                        }
                    }
                    if from.u != to_obj.u {
                        let unit = to_obj.u.clone().or_else(|| from.u.clone()).unwrap();
                        let convert = if to_obj.u.is_some() {
                            &mut from
                        } else {
                            &mut to_obj
                        };
                        convert_angle_unit(convert, &unit)?;
                    }
                    let update_duration = round(or_min_value(raw_number(&duration_value)), 12);
                    last_change_end = round(start_time + update_duration, 12);
                    let ease = parse_ease(&ease_spec(&ease_value)?)?;
                    let tween_id = TweenId(self.tweens.len());
                    self.tweens.push(Tween {
                        parent: id,
                        property: prop_name.clone(),
                        target,
                        value: Raw::Undefined,
                        ease,
                        from_number: from.n,
                        to_number: to_obj.n,
                        number: from.n,
                        unit: to_obj.u.clone(),
                        current_time: 0.0,
                        start_time,
                        delay: tween_delay,
                        update_duration,
                        change_duration: update_duration,
                        absolute_start_time,
                        absolute_update_start_time,
                        absolute_end_time: round(absolute_offset_time + last_change_end, 12),
                        has_from_value: has_from || is_from_to_array,
                        tween_type,
                        value_type: to_obj.t,
                        composition,
                        is_overlapped: false,
                        is_overridden: false,
                        render_transforms: false,
                        prev_rep: None,
                        next_rep: None,
                        prev: None,
                        next: None,
                    });
                    if composition != Composition::None {
                        self.compose_tween(tween_id);
                    }
                    let tween = &mut self.tweens[tween_id.0];
                    tween.value = match tween.value_type {
                        ValueType::Unit => Raw::String(format!(
                            "{}{}",
                            number_to_string(tween.to_number),
                            tween.unit.as_deref().unwrap_or("")
                        )),
                        ValueType::Number => Raw::Number(tween.to_number),
                    };
                    if first_change_start.is_nan() {
                        first_change_start = tween.start_time;
                    }
                    prev_tween = Some(tween_id);
                    animation_length += 1;
                    self.add_tween(id, tween_id);
                    tween_index += 1;
                }
                if iteration_delay.is_nan() || first_change_start < iteration_delay {
                    iteration_delay = first_change_start;
                }
                if iteration_duration.is_nan() || last_change_end > iteration_duration {
                    iteration_duration = last_change_end;
                }
                if tween_type == TweenType::Transform {
                    last_transform_group = Some((animation_length - tween_index, animation_length));
                }
            }
            if let Some((from, to)) = last_transform_group {
                for (i, tween) in self.tweens_of(id).into_iter().enumerate() {
                    if i >= from && i < to {
                        self.tweens[tween.0].render_transforms = true;
                    }
                }
            }
        }
        if parsed.is_empty() {
            return Err(Unsupported("no targets".into()));
        }
        if iteration_delay != 0.0 && !iteration_delay.is_nan() {
            for tween in self.tweens_of(id) {
                let tween = &mut self.tweens[tween.0];
                let at_start = tween.start_time - tween.delay;
                if at_start == 0.0 || at_start.is_nan() {
                    tween.delay -= iteration_delay;
                }
                tween.start_time -= iteration_delay;
            }
            iteration_duration -= iteration_delay;
        } else {
            iteration_delay = 0.0;
        }
        let timer = &mut self.timers[id.0];
        if iteration_duration == 0.0 || iteration_duration.is_nan() {
            iteration_duration = MIN_VALUE;
            timer.iteration_count = 0.0;
        }
        timer.targets = parsed;
        timer.id = self.js_animation_id;
        timer.duration = if iteration_duration == MIN_VALUE {
            MIN_VALUE
        } else {
            or_min_value(clamp_infinity(
                (iteration_duration + timer.loop_delay) * timer.iteration_count - timer.loop_delay,
            ))
        };
        timer.ease = parsed_playback_ease;
        timer.delay = iteration_delay;
        timer.iteration_duration = iteration_duration;
        Ok(id)
    }

    /// `composeTween(tween, siblings)` for replace composition.
    pub(crate) fn compose_tween(&mut self, tween: TweenId) {
        let key = (
            self.tweens[tween.0].target,
            self.tweens[tween.0].property.clone(),
        );
        let absolute_start_time = self.tweens[tween.0].absolute_start_time;
        // `addChild(siblings, tween, addTweenSortMethod, '_prevRep', '_nextRep')`
        let (head, tail) = *self.rep.entry(key.clone()).or_insert((None, None));
        let mut prev = tail;
        while let Some(p) = prev {
            let pt = &self.tweens[p.0];
            if pt.is_overridden || pt.absolute_start_time > absolute_start_time {
                prev = pt.prev_rep;
            } else {
                break;
            }
        }
        let next = match prev {
            Some(p) => self.tweens[p.0].next_rep,
            None => head,
        };
        let entry = self.rep.get_mut(&key).unwrap();
        match prev {
            Some(p) => self.tweens[p.0].next_rep = Some(tween),
            None => entry.0 = Some(tween),
        }
        match next {
            Some(n) => self.tweens[n.0].prev_rep = Some(tween),
            None => entry.1 = Some(tween),
        }
        self.tweens[tween.0].prev_rep = prev;
        self.tweens[tween.0].next_rep = next;

        let Some(prev_sibling) = prev else {
            return;
        };
        let tween_parent = self.tweens[tween.0].parent;
        let prev_parent = self.tweens[prev_sibling.0].parent;
        let prev_abs_end = self.tweens[prev_sibling.0].absolute_end_time;
        let pp = &self.timers[prev_parent.0];
        if self.timers[tween_parent.0].id != pp.id
            && pp.iteration_count > 1.0
            && prev_abs_end + (pp.duration - pp.iteration_duration) > absolute_start_time
        {
            override_tween(&mut self.tweens[prev_sibling.0]);
            let mut pps = self.tweens[prev_sibling.0].prev_rep;
            while let Some(s) = pps {
                if self.timers[self.tweens[s.0].parent.0].id != self.timers[prev_parent.0].id {
                    break;
                }
                override_tween(&mut self.tweens[s.0]);
                pps = self.tweens[s.0].prev_rep;
            }
        }
        let absolute_update_start_time = self.tweens[tween.0].absolute_update_start_time;
        if prev_abs_end > absolute_update_start_time {
            let ps = &mut self.tweens[prev_sibling.0];
            let prev_change_start = ps.start_time;
            let prev_tl_offset = prev_abs_end - (prev_change_start + ps.update_duration);
            let updated = round(
                absolute_update_start_time - prev_tl_offset - prev_change_start,
                12,
            );
            ps.change_duration = updated;
            ps.current_time = updated;
            ps.is_overlapped = true;
            if updated < MIN_VALUE {
                override_tween(ps);
            }
        }
        let tween_parent_tl = self.timers[tween_parent.0].parent;
        let prev_parent_tl = self.timers[prev_parent.0].parent;
        if tween_parent_tl.is_none() || tween_parent_tl != prev_parent_tl {
            let all_overlapped = |rt: &Runtime, animation: TimerId| {
                rt.tweens_of(animation)
                    .iter()
                    .all(|t| rt.tweens[t.0].is_overlapped)
            };
            if all_overlapped(self, prev_parent) {
                match prev_parent_tl {
                    Some(tl) => {
                        let pause_tl = self
                            .children(tl)
                            .into_iter()
                            .filter(|a| *a != prev_parent)
                            .all(|a| all_overlapped(self, a));
                        if pause_tl {
                            self.cancel(tl);
                        }
                    }
                    None => self.cancel(prev_parent),
                }
            }
        }
    }

    /// `removeTweenSliblings(tween)`.
    pub(crate) fn remove_tween_siblings(&mut self, tween: TweenId) {
        if self.tweens[tween.0].composition == Composition::None {
            return;
        }
        let key = (
            self.tweens[tween.0].target,
            self.tweens[tween.0].property.clone(),
        );
        let prev = self.tweens[tween.0].prev_rep;
        let next = self.tweens[tween.0].next_rep;
        let entry = self.rep.get_mut(&key).unwrap();
        match prev {
            Some(p) => self.tweens[p.0].next_rep = next,
            None => entry.0 = next,
        }
        match next {
            Some(n) => self.tweens[n.0].prev_rep = prev,
            None => entry.1 = prev,
        }
        self.tweens[tween.0].prev_rep = None;
        self.tweens[tween.0].next_rep = None;
    }
}

/// `overrideTween(tween)`.
fn override_tween(tween: &mut Tween) {
    tween.is_overlapped = true;
    tween.is_overridden = true;
    tween.change_duration = MIN_VALUE;
    tween.current_time = MIN_VALUE;
}

/// `decomposeTweenValue(tween, targetObject)`.
fn decompose_tween_value(tween: &Tween) -> Decomposed {
    Decomposed {
        t: tween.value_type,
        n: tween.to_number,
        u: tween.unit.clone(),
        o: None,
    }
}

/// `convertValueUnit` for angles; other units need layout.
fn convert_angle_unit(value: &mut Decomposed, unit: &str) -> Result<(), Unsupported> {
    if value.t == ValueType::Unit && value.u.as_deref() == Some(unit) {
        return Ok(());
    }
    let factor = |u: &str| match u {
        "deg" => Some(1.0),
        "rad" => Some(180.0 / PI),
        "turn" => Some(360.0),
        _ => None,
    };
    let current = value.u.clone().unwrap_or_default();
    match factor(&current) {
        Some(from) => {
            value.n = value.n * from / factor(unit).unwrap_or(f64::NAN);
            value.u = Some(unit.to_string());
            Ok(())
        }
        None => Err(Unsupported(format!(
            "converting {current:?} to {unit:?} needs layout, which the port does not read"
        ))),
    }
}

/// `+value` of a computed parameter.
fn raw_number(raw: &Raw) -> f64 {
    match raw {
        Raw::Undefined => f64::NAN,
        Raw::Number(n) => *n,
        Raw::String(s) => string_to_number(s),
    }
}

/// An `ease` parameter as an easing.
fn ease_spec(value: &Value) -> Result<EaseSpec, Unsupported> {
    match value {
        Value::String(s) => Ok(EaseSpec::Name(s.clone())),
        Value::Ease(spec) => Ok(spec.clone()),
        other => Err(Unsupported(format!("the ease {other:?}"))),
    }
}

/// `mergeObjects(o1, o2)`.
pub(crate) fn merge_objects(o1: &Obj, o2: &Obj) -> Obj {
    let mut merged = o1.clone();
    for (k, v) in &o2.0 {
        if o1.get(k).is_none() {
            merged.set(k, v.clone());
        }
    }
    merged
}

/// `generateKeyframes(keyframes, parameters)`.
fn generate_keyframes(keyframes: &Value, parameters: &Obj) -> Result<Obj, Unsupported> {
    let mut properties = Obj::default();
    match keyframes {
        Value::Array(frames) => {
            let objects: Vec<&Obj> = frames
                .iter()
                .map(|f| match f {
                    Value::Object(o) => Ok(o),
                    other => Err(Unsupported(format!("the keyframe {other:?}"))),
                })
                .collect::<Result<_, _>>()?;
            let names: Vec<String> = objects
                .iter()
                .flat_map(|o| o.keys().map(str::to_string).collect::<Vec<_>>())
                .filter(|k| is_key(k))
                .collect();
            for name in names {
                let array = objects
                    .iter()
                    .map(|key| {
                        let mut new_key = Obj::default();
                        for (p, v) in &key.0 {
                            if is_key(p) {
                                if *p == name {
                                    new_key.set("to", v.clone());
                                }
                            } else {
                                new_key.set(p, v.clone());
                            }
                        }
                        Value::Object(new_key)
                    })
                    .collect();
                properties.set(&name, Value::Array(array));
            }
        }
        Value::Object(frames) => {
            let total = to_number_value(set_value(
                parameters.get("duration"),
                &Value::Number(1000.0),
            ));
            let mut keys: Vec<(f64, &Obj)> = frames
                .0
                .iter()
                .map(|(k, v)| match v {
                    Value::Object(o) => Ok((parse_float(k) / 100.0, o)),
                    other => Err(Unsupported(format!("the keyframe {other:?}"))),
                })
                .collect::<Result<_, _>>()?;
            // `Array.prototype.sort` is stable.
            keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            let mut arrays: Vec<(String, Vec<Obj>)> = Vec::new();
            for (offset, prop) in keys {
                for (name, value) in &prop.0 {
                    if !is_key(name) {
                        continue;
                    }
                    let index = match arrays.iter().position(|(n, _)| n == name) {
                        Some(i) => i,
                        None => {
                            arrays.push((name.clone(), Vec::new()));
                            arrays.len() - 1
                        }
                    };
                    let array = &mut arrays[index].1;
                    let duration = offset * total;
                    let length = array.len();
                    let mut key = Obj(vec![("to".into(), value.clone())]);
                    let progress: f64 = array
                        .iter()
                        .map(|k| to_number_value(k.get("duration").unwrap_or(&Value::Null)))
                        .sum();
                    if length == 1 {
                        key.set("from", array[0].get("to").cloned().unwrap_or(Value::Null));
                    }
                    if let Some(ease) = prop.get("ease") {
                        if truthy(ease) {
                            key.set("ease", ease.clone());
                        }
                    }
                    key.set(
                        "duration",
                        Value::Number(duration - if length > 0 { progress } else { 0.0 }),
                    );
                    array.push(key);
                }
            }
            for (name, mut array) in arrays {
                let mut prev_ease: Option<Value> = None;
                for key in array.iter_mut() {
                    let current = key.get("ease").cloned();
                    key.0.retain(|(k, _)| k != "ease");
                    if let Some(e) = prev_ease.take() {
                        key.set("ease", e);
                    }
                    prev_ease = current;
                }
                let first_duration = array[0].get("duration").map_or(0.0, to_number_value);
                if first_duration == 0.0 || first_duration.is_nan() {
                    array.remove(0);
                }
                properties.set(
                    &name,
                    Value::Array(array.into_iter().map(Value::Object).collect()),
                );
            }
        }
        other => return Err(Unsupported(format!("keyframes {other:?}"))),
    }
    Ok(properties)
}
