//! Motion's `animate(sequence, options)` (framer-motion 14.14.0
//! `animation/sequence/create.mjs` and its utils): a timeline of segments
//! becomes one animation per element and value, with absolute times turned
//! into keyframe offsets and per-keyframe easings.
//!
//! The output is what `animateSequence` hands to `animateSubject`. Each
//! animation then takes the single-value path in [`super::native`].

use super::easing::Ease;
use super::generator::create_generator_easing;
use super::js::{mix_number, parse_float, progress, seconds_to_milliseconds, wrap};
use super::native::{AcceleratedValue, EaseDefinition, RepeatType, Transition, TransitionType};
use super::spring::SpringOptions;
use super::stagger::Stagger;
use div::prelude::*;
use div::typed_style::{TransformList, TransformOperation};
use std::cmp::Ordering;
use std::collections::HashMap;

/// One keyframe value.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    Opacity(f64),
    Transform(TransformList),
}

/// The keyframes of one value in a segment. A single frame is a target
/// that animates from the value's current state.
#[derive(Debug, Clone)]
pub struct ValueKeyframes {
    pub property: AcceleratedValue,
    pub frames: Vec<Frame>,
}

/// A segment's or label's `at`: seconds, or one of Motion's strings
/// (`"+0.5"`, `"-0.2"`, `"<"`, `"<0.1"`, `"label"`, `"label+0.3"`).
#[derive(Debug, Clone, PartialEq)]
pub enum At {
    Seconds(f64),
    Text(String),
}

/// A segment's `delay`: seconds, or `stagger()`.
#[derive(Debug, Clone)]
pub enum Delay {
    Seconds(f64),
    Stagger(Stagger),
}

/// A segment's transition: `at`, `delay`, and the rest of the transition.
/// `transition.delay` is not read; use `delay`.
#[derive(Debug, Clone, Default)]
pub struct SegmentTransition {
    pub at: Option<At>,
    pub delay: Option<Delay>,
    pub transition: Transition,
}

/// A segment's subject: element indices.
#[derive(Debug, Clone)]
pub enum Subject {
    One(usize),
    Many(Vec<usize>),
}

#[derive(Debug, Clone)]
pub enum SequenceItem {
    /// `"name"`: a label at the current time.
    Label(String),
    /// `{ name, at }`.
    LabelAt { name: String, at: Option<At> },
    /// `{ sequence, at }`.
    Nested {
        sequence: Vec<SequenceItem>,
        at: Option<At>,
    },
    /// `[subject, keyframes, transition]`.
    Segment {
        subject: Subject,
        values: Vec<ValueKeyframes>,
        transition: Box<SegmentTransition>,
    },
}

/// `animate(sequence, { defaultTransition, ...sequenceTransition })`.
#[derive(Debug, Clone, Default)]
pub struct SequenceOptions {
    pub default_transition: Transition,
    /// Spread over every value's transition last (`repeat`, `delay`, ...).
    pub transition: Transition,
}

/// One value's animation: keyframes with wildcards (`None`, filled when the
/// animation starts) and the transition `animateSubject` receives.
#[derive(Debug, Clone)]
pub struct SequenceAnimation {
    pub element: usize,
    pub property: AcceleratedValue,
    pub frames: Vec<Option<Frame>>,
    pub transition: Transition,
}

/// `MAX_REPEAT`: segments repeat only below this count.
const MAX_REPEAT: f64 = 20.0;

struct Entry {
    at: f64,
    value: Option<Frame>,
    easing: Option<Ease>,
}

type ValueSequences = Vec<(AcceleratedValue, Vec<Entry>)>;

struct Builder<'a> {
    default_transition: &'a Transition,
    default_duration: f64,
    prev_time: f64,
    current_time: f64,
    total_duration: f64,
    labels: HashMap<String, f64>,
    /// Per element, in first-use order; per value, in first-use order.
    sequences: Vec<(usize, ValueSequences)>,
}

/// `createAnimationsFromSequence(sequence, options)`.
pub fn create_animations_from_sequence(
    sequence: &[SequenceItem],
    options: &SequenceOptions,
) -> Vec<SequenceAnimation> {
    let default_transition = &options.default_transition;
    let mut builder = Builder {
        default_transition,
        // `defaultTransition.duration || 0.3`
        default_duration: default_transition
            .duration
            .filter(|d| *d != 0.0 && !d.is_nan())
            .unwrap_or(0.3),
        prev_time: 0.0,
        current_time: 0.0,
        total_duration: 0.0,
        labels: HashMap::new(),
        sequences: Vec::new(),
    };
    builder.resolve_sequence(sequence, 0.0);
    let total_duration = builder.total_duration;
    // `{ type, ...remainingDefaultTransition } = defaultTransition`
    let mut remaining_default = default_transition.clone();
    remaining_default.kind = None;
    let mut animations = Vec::new();
    for (element, values) in builder.sequences {
        for (property, mut entries) in values {
            entries.sort_by(compare_by_time);
            let mut frames = Vec::with_capacity(entries.len() + 2);
            let mut offsets = Vec::with_capacity(entries.len() + 2);
            let mut easings = Vec::with_capacity(entries.len() + 1);
            for entry in entries {
                frames.push(entry.value);
                offsets.push(progress(0.0, total_duration, entry.at));
                easings.push(entry.easing.unwrap_or(Ease::EaseOut));
            }
            if offsets[0] != 0.0 {
                offsets.insert(0, 0.0);
                frames.insert(0, frames[0].clone());
                easings.insert(0, Ease::EaseInOut);
            }
            if offsets[offsets.len() - 1] != 1.0 {
                offsets.push(1.0);
                frames.push(None);
            }
            let mut transition = remaining_default.clone();
            transition.duration = Some(total_duration);
            transition.ease = Some(EaseDefinition::PerSegment(easings));
            transition.times = Some(offsets);
            overlay(&mut transition, &options.transition);
            animations.push(SequenceAnimation {
                element,
                property,
                frames,
                transition,
            });
        }
    }
    animations
}

impl Builder<'_> {
    fn resolve_time(&self, at: &At, base: f64) -> f64 {
        match at {
            At::Seconds(seconds) => base + seconds,
            At::Text(text) => calc_next_time(self.current_time, text, self.prev_time, &self.labels),
        }
    }

    /// `resolveSequence(sequence, base)`: returns the time it ends.
    fn resolve_sequence(&mut self, sequence: &[SequenceItem], base: f64) -> f64 {
        let mut sequence_end = base;
        for item in sequence {
            match item {
                SequenceItem::Label(name) => {
                    self.labels.insert(name.clone(), self.current_time);
                }
                SequenceItem::LabelAt { name, at } => {
                    let start = match at {
                        None => self.current_time,
                        Some(at) => self.resolve_time(at, base),
                    };
                    self.labels.insert(name.clone(), start);
                }
                SequenceItem::Nested { sequence, at } => {
                    let start = match at {
                        None => self.current_time,
                        Some(at) => self.resolve_time(at, base),
                    };
                    self.prev_time = start;
                    self.current_time = start;
                    self.current_time = self.resolve_sequence(sequence, start);
                    self.prev_time = start;
                    sequence_end = sequence_end.max(self.current_time);
                }
                SequenceItem::Segment {
                    subject,
                    values,
                    transition,
                } => {
                    if let Some(at) = &transition.at {
                        self.current_time = self.resolve_time(at, base);
                    }
                    let mut max_duration: f64 = 0.0;
                    let subjects = match subject {
                        Subject::One(index) => vec![*index],
                        Subject::Many(indices) => indices.clone(),
                    };
                    let count = subjects.len();
                    for (subject_index, element) in subjects.into_iter().enumerate() {
                        for value in values {
                            let duration = self.resolve_value_sequence(
                                element,
                                value,
                                transition,
                                subject_index,
                                count,
                            );
                            max_duration = max_duration.max(duration);
                        }
                    }
                    self.prev_time = self.current_time;
                    self.current_time += max_duration;
                    sequence_end = sequence_end.max(self.current_time);
                }
            }
        }
        sequence_end
    }

    fn value_sequence(&mut self, element: usize, property: AcceleratedValue) -> &mut Vec<Entry> {
        let element_position = match self.sequences.iter().position(|(e, _)| *e == element) {
            Some(position) => position,
            None => {
                self.sequences.push((element, Vec::new()));
                self.sequences.len() - 1
            }
        };
        let values = &mut self.sequences[element_position].1;
        let value_position = match values.iter().position(|(p, _)| *p == property) {
            Some(position) => position,
            None => {
                values.push((property, Vec::new()));
                values.len() - 1
            }
        };
        &mut values[value_position].1
    }

    /// `resolveValueSequence`: returns `calculatedDelay + duration`, which
    /// the segment's `maxDuration` takes the maximum of.
    fn resolve_value_sequence(
        &mut self,
        element: usize,
        value: &ValueKeyframes,
        segment: &SegmentTransition,
        element_index: usize,
        subject_count: usize,
    ) -> f64 {
        let transition = &segment.transition;
        let default = self.default_transition;
        let mut keyframes: Vec<Option<Frame>> = value.frames.iter().cloned().map(Some).collect();
        let mut times = transition
            .times
            .clone()
            .unwrap_or_else(|| default_offset(keyframes.len()));
        // `type = defaultTransition.type || "keyframes"`
        let kind = transition.kind.or(default.kind);
        let repeat_delay = transition.repeat_delay.unwrap_or(0.0);
        // `ease = defaultTransition.ease || "easeOut"`
        let mut ease = transition.ease.clone().unwrap_or_else(|| {
            default
                .ease
                .clone()
                .unwrap_or(EaseDefinition::One(Ease::EaseOut))
        });
        let mut duration = transition.duration;
        let calculated_delay = match &segment.delay {
            None => 0.0,
            Some(Delay::Seconds(seconds)) => *seconds,
            Some(Delay::Stagger(stagger)) => stagger.delay(element_index, subject_count),
        };
        let count = keyframes.len();
        // Only `type: "spring"` has a generator here (`{ spring }`).
        if count <= 2 && kind == Some(TransitionType::Spring) {
            let absolute_delta = match value.frames.as_slice() {
                [Frame::Opacity(from), Frame::Opacity(to)] => (to - from).abs(),
                _ => 100.0,
            };
            // `{ ...defaultTransition, ...remainingTransition }`, then the
            // value's own duration in milliseconds. A default duration is
            // passed through in seconds, as Motion does.
            let pick = |own: Option<f64>, fallback: Option<f64>| own.or(fallback);
            let spring = SpringOptions {
                stiffness: pick(transition.stiffness, default.stiffness),
                damping: pick(transition.damping, default.damping),
                mass: pick(transition.mass, default.mass),
                duration: match transition.duration {
                    Some(seconds) => Some(seconds_to_milliseconds(seconds)),
                    None => default.duration,
                },
                bounce: pick(transition.bounce, default.bounce),
                visual_duration: pick(transition.visual_duration, default.visual_duration),
                velocity: None,
                rest_speed: pick(transition.rest_speed, default.rest_speed),
                rest_delta: pick(transition.rest_delta, default.rest_delta),
            };
            let (function, seconds) = create_generator_easing(&spring, absolute_delta, f64::NAN);
            ease = EaseDefinition::One(Ease::Function(function));
            duration = Some(seconds);
        }
        let mut duration = duration.unwrap_or(self.default_duration);
        let start_time = self.current_time + calculated_delay;
        if times.len() == 1 && times[0] == 0.0 {
            times.push(1.0);
        }
        if times.len() > keyframes.len() {
            let remainder = times.len() - keyframes.len();
            fill_offset(&mut times, remainder);
        }
        if keyframes.len() == 1 {
            keyframes.insert(0, None);
        }
        if let Some(repeat) = transition
            .repeat
            .filter(|r| *r != 0.0 && !r.is_nan() && *r < MAX_REPEAT)
        {
            let repeat_delay_units = if duration > 0.0 {
                repeat_delay / duration
            } else {
                0.0
            };
            duration = duration * (repeat + 1.0) + repeat_delay * repeat;
            let original_keyframes = keyframes.clone();
            let original_times = times.clone();
            // `Array.isArray(ease) ? [...ease] : [ease]`. Motion would spread
            // a single Bézier array into its four numbers here; that case is
            // kept as one Bézier easing and is not in the recorded cases.
            let mut eases = match ease {
                EaseDefinition::One(one) => vec![one],
                EaseDefinition::PerSegment(segments) => segments,
            };
            let original_ease = eases.clone();
            let flipping = matches!(
                transition.repeat_type,
                Some(RepeatType::Reverse | RepeatType::Mirror)
            );
            let (flipped_keyframes, flipped_eases) = if flipping {
                let mut reversed = original_keyframes.clone();
                reversed.reverse();
                let eases = if transition.repeat_type == Some(RepeatType::Reverse) {
                    original_ease
                        .iter()
                        .rev()
                        .map(Ease::reversed_if_function)
                        .collect()
                } else {
                    original_ease.clone()
                };
                (reversed, eases)
            } else {
                (original_keyframes.clone(), original_ease.clone())
            };
            let mut repeat_index = 0.0;
            while repeat_index < repeat {
                let flipped = flipping && repeat_index % 2.0 == 0.0;
                let (iteration_keyframes, iteration_ease) = if flipped {
                    (&flipped_keyframes, &flipped_eases)
                } else {
                    (&original_keyframes, &original_ease)
                };
                let iteration_start = (repeat_index + 1.0) * (1.0 + repeat_delay_units);
                if repeat_delay_units > 0.0 {
                    keyframes.push(keyframes[keyframes.len() - 1].clone());
                    times.push(iteration_start);
                    eases.push(Ease::Linear);
                }
                keyframes.extend(iteration_keyframes.iter().cloned());
                for index in 0..iteration_keyframes.len() {
                    times.push(
                        original_times.get(index).copied().unwrap_or(f64::NAN) + iteration_start,
                    );
                    eases.push(if index == 0 {
                        Ease::Linear
                    } else {
                        easing_for_segment(
                            &EaseDefinition::PerSegment(iteration_ease.clone()),
                            index - 1,
                        )
                        .unwrap_or(Ease::EaseOut)
                    });
                }
                repeat_index += 1.0;
            }
            // normalizeTimes
            let total_units = repeat + 1.0 + repeat * repeat_delay_units;
            for time in &mut times {
                *time /= total_units;
            }
            ease = EaseDefinition::PerSegment(eases);
        }
        let target_time = start_time + duration;
        let sequence = self.value_sequence(element, value.property);
        add_keyframes(sequence, keyframes, &ease, &times, start_time, target_time);
        self.total_duration = self.total_duration.max(target_time);
        calculated_delay + duration
    }
}

/// `getEasingForSegment(easing, i)`.
fn easing_for_segment(ease: &EaseDefinition, index: usize) -> Option<Ease> {
    match ease {
        EaseDefinition::One(one) => Some(one.clone()),
        EaseDefinition::PerSegment(segments) if segments.is_empty() => None,
        EaseDefinition::PerSegment(segments) => {
            let position = wrap(0.0, segments.len() as f64, index as f64) as usize;
            Some(segments[position].clone())
        }
    }
}

/// `addKeyframes`: erase what lies strictly between the segment's start and
/// end, then add its keyframes at absolute times.
fn add_keyframes(
    sequence: &mut Vec<Entry>,
    keyframes: Vec<Option<Frame>>,
    ease: &EaseDefinition,
    offsets: &[f64],
    start_time: f64,
    end_time: f64,
) {
    sequence.retain(|entry| !(entry.at > start_time && entry.at < end_time));
    for (index, value) in keyframes.into_iter().enumerate() {
        sequence.push(Entry {
            value,
            at: mix_number(
                start_time,
                end_time,
                offsets.get(index).copied().unwrap_or(f64::NAN),
            ),
            easing: easing_for_segment(ease, index),
        });
    }
}

/// `compareByTime`: by time, wildcards last among equal times.
fn compare_by_time(a: &Entry, b: &Entry) -> Ordering {
    if a.at == b.at {
        match (&a.value, &b.value) {
            (None, None) | (Some(_), Some(_)) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
        }
    } else {
        (a.at - b.at).partial_cmp(&0.0).unwrap_or(Ordering::Equal)
    }
}

/// motion-dom `defaultOffset(arr)`.
fn default_offset(count: usize) -> Vec<f64> {
    let mut offset = vec![0.0];
    fill_offset(&mut offset, count.saturating_sub(1));
    offset
}

/// motion-dom `fillOffset(offset, remaining)`.
fn fill_offset(offset: &mut Vec<f64>, remaining: usize) {
    let min = offset[offset.len() - 1];
    for index in 1..=remaining {
        let offset_progress = progress(0.0, remaining as f64, index as f64);
        offset.push(mix_number(min, 1.0, offset_progress));
    }
}

/// `calcLabelTime(position, labels)`: a label, or `label+n` / `label-n`.
fn calc_label_time(position: &str, labels: &HashMap<String, f64>) -> Option<f64> {
    if let Some(time) = labels.get(position) {
        return Some(*time);
    }
    // `/^(.+?)([+-][\d.]+)$/`: the shortest name followed by a signed number.
    let bytes = position.as_bytes();
    let split = (1..bytes.len()).find(|&index| {
        matches!(bytes[index], b'+' | b'-')
            && index + 1 < bytes.len()
            && bytes[index + 1..]
                .iter()
                .all(|b| b.is_ascii_digit() || *b == b'.')
    })?;
    let (name, offset) = position.split_at(split);
    labels
        .get(name)
        .map(|time| (time + parse_float(offset)).max(0.0))
}

/// `calcNextTime(current, next, prev, labels)` for a string `next`.
fn calc_next_time(current: f64, next: &str, prev: f64, labels: &HashMap<String, f64>) -> f64 {
    if next.starts_with('-') || next.starts_with('+') {
        (current + parse_float(next)).max(0.0)
    } else if next == "<" {
        prev
    } else if let Some(rest) = next.strip_prefix('<') {
        (prev + parse_float(rest)).max(0.0)
    } else {
        calc_label_time(next, labels).unwrap_or(current)
    }
}

/// Every `Some` field of `top` replaces the field in `base` (an object
/// spread of defined keys).
fn overlay(base: &mut Transition, top: &Transition) {
    macro_rules! take {
        ($($field:ident),*) => {$(
            if top.$field.is_some() {
                base.$field = top.$field.clone();
            }
        )*};
    }
    take!(
        kind,
        duration,
        delay,
        ease,
        times,
        repeat,
        repeat_type,
        repeat_delay,
        stiffness,
        damping,
        mass,
        bounce,
        visual_duration,
        rest_speed,
        rest_delta
    );
}

/// What the value is before the animation: what Motion's keyframe resolver
/// reads for a leading wildcard.
#[derive(Debug, Clone, PartialEq)]
pub enum Underlying {
    Opacity(f64),
    /// `transform: none`.
    NoTransform,
}

/// The keyframes an animation starts with: a leading wildcard takes the
/// underlying value (`readKeyframes`; `none` becomes the target's
/// animatable none, every number zero), and every later wildcard the value
/// before it (`fillWildcards`).
pub fn fill_wildcards(
    frames: &[Option<Frame>],
    underlying: &Underlying,
) -> Result<Vec<Frame>, String> {
    let first = match &frames[0] {
        Some(frame) => frame.clone(),
        None => match underlying {
            Underlying::Opacity(opacity) => Frame::Opacity(*opacity),
            Underlying::NoTransform => {
                let template = frames
                    .iter()
                    .flatten()
                    .find_map(|frame| match frame {
                        Frame::Transform(list) => Some(list),
                        Frame::Opacity(_) => None,
                    })
                    .ok_or("no transform keyframe to take an animatable none from")?;
                Frame::Transform(animatable_none(template)?)
            }
        },
    };
    let mut filled = vec![first];
    for frame in &frames[1..] {
        let next = frame
            .clone()
            .unwrap_or_else(|| filled[filled.len() - 1].clone());
        filled.push(next);
    }
    Ok(filled)
}

/// `getAnimatableNone` for a transform string: the same functions with
/// every number set to zero.
fn animatable_none(template: &TransformList) -> Result<TransformList, String> {
    template
        .0
        .iter()
        .map(|operation| match operation {
            TransformOperation::Translate(_, _) => Ok(TransformOperation::Translate(
                LengthValue::px(0.0),
                LengthValue::px(0.0),
            )),
            TransformOperation::Rotate(_) => Ok(TransformOperation::Rotate(0.0)),
            TransformOperation::Scale(_, _) => Ok(TransformOperation::Scale(0.0, 0.0)),
            other => Err(format!("animatable none of {other:?} is not ported")),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(TransformList)
}
