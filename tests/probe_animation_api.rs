//! Probe: what Div's public animation API does at the pinned commit.
//!
//! Every test asserts what Div does today. A test whose name starts with
//! `gap_` pins a behaviour that differs from CSS/Chromium or from what a
//! motion library needs; `docs/design.md` explains each one. When Div fixes
//! a gap, the matching test fails and the design table needs updating.

use div::prelude::*;
use div::typed_style::{
    AnimationOptions, CompositeOperation, Easing, FillMode, IterationCount, Keyframes, LinearStop,
    RendererStyleValue, StepPosition, TransformList, TransformOperation, Visibility,
};
use div::AnimationEventKind;
use std::cell::RefCell;
use std::rc::Rc;

fn document() -> Document {
    let document = Document::with_font_collection(
        ViewportMetrics::from_logical_size(400.0, 200.0, 1.0).unwrap(),
        FontCollection::deterministic_test(),
    )
    .unwrap();
    document.body().set_background_color(Color::WHITE).unwrap();
    document
}

fn boxed(document: &Document) -> Element {
    let node = Element::create(document, "div").unwrap();
    node.apply_style(&style! {
        position: absolute; left: 20px; top: 20px; width: 50px; height: 50px
    })
    .unwrap();
    node.set_background_color(Color::BLUE).unwrap();
    document.body().append_child(&node).unwrap();
    node
}

fn linear(duration_ms: f64) -> AnimationOptions {
    AnimationOptions {
        duration_ms,
        fill: FillMode::Both,
        easing: Easing::Linear,
        ..AnimationOptions::default()
    }
}

fn eased(easing: Easing) -> AnimationOptions {
    AnimationOptions {
        easing,
        ..linear(1000.0)
    }
}

fn opacity_at(easing: Easing, time_ms: f64) -> f32 {
    let document = document();
    let node = boxed(&document);
    node.animate(
        StyleProperty::Opacity,
        Keyframes::from_values(0.0_f32, 1.0_f32),
        eased(easing),
    )
    .unwrap();
    document.advance_time(time_ms).unwrap();
    node.computed_style().unwrap().opacity
}

fn visibility(value: Visibility) -> StyleValue {
    StyleValue::Renderer(RendererStyleValue::Visibility(value))
}

fn close(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 1e-3
}

// ── Properties ──────────────────────────────────────────────────────────

#[test]
fn opacity_color_and_width_interpolate_on_the_manual_clock() {
    let document = document();
    let node = boxed(&document);
    node.animate(
        StyleProperty::Opacity,
        Keyframes::from_values(0.0_f32, 1.0_f32),
        linear(100.0),
    )
    .unwrap();
    node.animate(
        StyleProperty::BackgroundColor,
        Keyframes::from_values(
            Color::from_rgba8(255, 0, 0, 255),
            Color::from_rgba8(0, 0, 255, 255),
        ),
        linear(100.0),
    )
    .unwrap();
    node.animate(
        StyleProperty::Width,
        Keyframes::from_values(LengthValue::px(50.0), LengthValue::px(150.0)),
        linear(100.0),
    )
    .unwrap();
    document.advance_time(25.0).unwrap();
    let style = node.computed_style().unwrap();
    assert!(close(style.opacity, 0.25), "opacity {}", style.opacity);
    let color = style.background_color;
    assert!(close(color.r, 0.75) && close(color.b, 0.25), "{color:?}");
    let rect = node.bounding_rect().unwrap().unwrap();
    assert_eq!(rect.width, 75.0);
}

#[test]
fn transform_animation_moves_the_layout_box_read_by_bounding_rect() {
    let document = document();
    let node = boxed(&document);
    let from = TransformList(vec![TransformOperation::Translate(
        LengthValue::px(0.0),
        LengthValue::px(0.0),
    )]);
    let to = TransformList(vec![TransformOperation::Translate(
        LengthValue::px(100.0),
        LengthValue::px(0.0),
    )]);
    node.animate(
        StyleProperty::Transform,
        Keyframes::from_values(from, to),
        linear(100.0),
    )
    .unwrap();
    document.advance_time(50.0).unwrap();
    // bounding_rect is post-transform, which FLIP needs.
    assert_eq!(node.bounding_rect().unwrap().unwrap().x, 70.0);
}

#[test]
fn gap_no_css_animation_transition_or_motion_properties() {
    for name in [
        "transition",
        "transition-property",
        "transition-duration",
        "animation",
        "animation-name",
        "animation-duration",
        "will-change",
        "translate",
        "rotate",
        "scale",
        "filter",
        "box-shadow-color",
    ] {
        assert_eq!(StyleProperty::from_css_name(name), None, "{name}");
    }
    // What exists: transform, opacity and per-function filters.
    for name in ["transform", "opacity", "filter-blur", "transform-origin"] {
        assert!(StyleProperty::from_css_name(name).is_some(), "{name}");
    }
}

#[test]
fn gap_setting_a_style_never_starts_a_transition() {
    // There is no `transition` property, so a plain style change jumps.
    let document = document();
    let node = boxed(&document);
    document.advance_time(0.0).unwrap();
    node.set_property(StyleProperty::Opacity, 0.0_f32.into())
        .unwrap();
    document.advance_time(16.0).unwrap();
    assert_eq!(node.computed_style().unwrap().opacity, 0.0);
    assert!(!document.is_animating().unwrap());
}

#[test]
fn gap_visibility_is_discrete_at_half_way() {
    // CSS: visibility interpolates as "visible if either end is visible" for
    // 0 < p < 1 (css-transitions-1 §animatable types; Chromium follows it).
    // Div flips discrete values at p = 0.5.
    let document = document();
    let node = boxed(&document);
    node.animate(
        StyleProperty::Visibility,
        Keyframes::from_values(
            visibility(Visibility::Hidden),
            visibility(Visibility::Visible),
        ),
        linear(100.0),
    )
    .unwrap();
    document.advance_time(10.0).unwrap();
    assert_eq!(
        node.computed_style().unwrap().visibility,
        Visibility::Hidden
    );
    document.advance_time(60.0).unwrap();
    assert_eq!(
        node.computed_style().unwrap().visibility,
        Visibility::Visible
    );
}

// ── Easing ──────────────────────────────────────────────────────────────

#[test]
fn cubic_bezier_ease_matches_the_css_curve() {
    // ease = cubic-bezier(0.25, 0.1, 0.25, 1); y(0.5) = 0.8024 (Chromium
    // gfx::CubicBezier). Div has no named `ease` constant.
    let ease = Easing::cubic_bezier(0.25, 0.1, 0.25, 1.0).unwrap();
    let value = opacity_at(ease, 500.0);
    assert!((value - 0.8024).abs() < 2e-3, "{value}");
}

#[test]
fn steps_support_every_jump_position() {
    let at = |count, position| opacity_at(Easing::steps(count, position).unwrap(), 300.0);
    assert!(close(at(4, StepPosition::JumpEnd), 0.25));
    assert!(close(at(4, StepPosition::JumpStart), 0.5));
    assert!(close(at(4, StepPosition::JumpNone), 1.0 / 3.0));
    assert!(close(at(4, StepPosition::JumpBoth), 0.4));
}

#[test]
fn linear_function_with_stops_is_supported() {
    // linear(0, 0.25 75%, 1): at 50% the output is 0.25 * (50 / 75).
    let stops = vec![
        LinearStop {
            input: 0.0,
            output: 0.0,
        },
        LinearStop {
            input: 0.75,
            output: 0.25,
        },
        LinearStop {
            input: 1.0,
            output: 1.0,
        },
    ];
    let value = opacity_at(Easing::linear_stops(stops).unwrap(), 500.0);
    assert!(close(value, 0.25 * 50.0 / 75.0), "{value}");
}

// ── Timing, fill and control ────────────────────────────────────────────

#[test]
fn delay_fill_iterations_and_direction_follow_css_timing() {
    let document = document();
    let node = boxed(&document);
    node.animate(
        StyleProperty::Opacity,
        Keyframes::from_values(0.0_f32, 1.0_f32),
        AnimationOptions {
            delay_ms: 100.0,
            iterations: IterationCount::Number(2.5),
            direction: div::typed_style::PlaybackDirection::Alternate,
            ..linear(100.0)
        },
    )
    .unwrap();
    document.advance_time(50.0).unwrap(); // backwards fill
    assert!(close(node.computed_style().unwrap().opacity, 0.0));
    document.advance_time(225.0).unwrap(); // second iteration, reversed
    assert!(close(node.computed_style().unwrap().opacity, 0.75));
    document.advance_time(400.0).unwrap(); // forwards fill at 2.5 iterations
    assert!(close(node.computed_style().unwrap().opacity, 0.5));
}

#[test]
fn pause_seek_rate_and_cancel_are_available_by_id() {
    let document = document();
    let node = boxed(&document);
    let id = node
        .animate(
            StyleProperty::Opacity,
            Keyframes::from_values(0.0_f32, 1.0_f32),
            linear(100.0),
        )
        .unwrap();
    document.advance_time(20.0).unwrap();
    document.pause_animation(id).unwrap();
    document.advance_time(80.0).unwrap();
    assert!(close(node.computed_style().unwrap().opacity, 0.2));
    document.seek_animation(id, 60.0).unwrap();
    document.set_animation_playback_rate(id, -1.0).unwrap();
    document.play_animation(id).unwrap();
    document.advance_time(100.0).unwrap();
    assert!(close(node.computed_style().unwrap().opacity, 0.4));
    document.cancel_animation(id).unwrap();
    assert!(document.animation_state(id).is_err());
    assert!(close(node.computed_style().unwrap().opacity, 1.0));
}

#[test]
fn additive_composition_stacks_transforms() {
    let document = document();
    let node = boxed(&document);
    let shift = |x| {
        TransformList(vec![TransformOperation::Translate(
            LengthValue::px(x),
            LengthValue::px(0.0),
        )])
    };
    node.animate(
        StyleProperty::Transform,
        Keyframes::from_values(shift(0.0), shift(100.0)),
        linear(100.0),
    )
    .unwrap();
    node.animate(
        StyleProperty::Transform,
        Keyframes::from_values(shift(10.0), shift(10.0)),
        AnimationOptions {
            composite: CompositeOperation::Add,
            ..linear(100.0)
        },
    )
    .unwrap();
    document.advance_time(50.0).unwrap();
    assert_eq!(node.bounding_rect().unwrap().unwrap().x, 80.0);
}

// ── Events ──────────────────────────────────────────────────────────────

#[test]
fn animation_events_dispatch_but_carry_no_details() {
    let document = document();
    let node = boxed(&document);
    let seen = Rc::new(RefCell::new(Vec::new()));
    for name in ["animationstart", "animationiteration", "animationend"] {
        let seen = seen.clone();
        node.on(name, move |event: &Event| {
            seen.borrow_mut().push(event.event_type.clone())
        })
        .unwrap();
    }
    node.animate(
        StyleProperty::Opacity,
        Keyframes::from_values(0.0_f32, 1.0_f32),
        AnimationOptions {
            iterations: IterationCount::Number(2.0),
            ..linear(100.0)
        },
    )
    .unwrap();
    document.advance_time(1.0).unwrap();
    document.advance_time(150.0).unwrap();
    document.advance_time(250.0).unwrap();
    assert_eq!(
        *seen.borrow(),
        ["animationstart", "animationiteration", "animationend"]
    );
    // Details (property, elapsed time) only come from the drained queue.
    let drained = document.drain_animation_events().unwrap();
    let kinds: Vec<_> = drained.iter().map(|event| event.kind).collect();
    assert_eq!(
        kinds,
        [
            AnimationEventKind::Start,
            AnimationEventKind::Iteration,
            AnimationEventKind::End
        ]
    );
    assert_eq!(drained[2].elapsed_time_ms, 200.0);
}

#[test]
fn gap_transitions_fire_animation_events_not_transition_events() {
    let document = document();
    let node = boxed(&document);
    let seen = Rc::new(RefCell::new(Vec::new()));
    for name in [
        "transitionrun",
        "transitionstart",
        "transitionend",
        "animationend",
    ] {
        let seen = seen.clone();
        node.on(name, move |event: &Event| {
            seen.borrow_mut().push(event.event_type.clone())
        })
        .unwrap();
    }
    document.advance_time(0.0).unwrap();
    node.transition(StyleProperty::Opacity, 0.0_f32, linear(100.0))
        .unwrap();
    document.advance_time(50.0).unwrap();
    document.advance_time(150.0).unwrap();
    assert_eq!(*seen.borrow(), ["animationend"]);
}

// ── Interruption (what springs and gestures need) ───────────────────────

#[test]
fn interrupting_a_transition_starts_from_the_current_value_without_velocity() {
    let document = document();
    let node = boxed(&document);
    document.advance_time(0.0).unwrap();
    node.transition(StyleProperty::Width, LengthValue::px(150.0), linear(100.0))
        .unwrap();
    document.advance_time(50.0).unwrap();
    assert_eq!(node.bounding_rect().unwrap().unwrap().width, 100.0);
    // Retarget mid-flight: the new transition starts at the sampled 100px.
    node.transition(StyleProperty::Width, LengthValue::px(50.0), linear(100.0))
        .unwrap();
    document.advance_time(50.0).unwrap();
    assert_eq!(node.bounding_rect().unwrap().unwrap().width, 100.0);
    document.advance_time(100.0).unwrap();
    assert_eq!(node.bounding_rect().unwrap().unwrap().width, 75.0);
    // Velocity is not carried: the curve restarts from rest (linear here),
    // so a spring cannot be retargeted through this API without a jolt.
}

// ── Accessibility ───────────────────────────────────────────────────────

#[test]
fn reduced_motion_is_a_manual_document_flag_that_jumps_to_the_end() {
    let document = document();
    let node = boxed(&document);
    assert!(!document.prefers_reduced_motion().unwrap());
    document.set_prefers_reduced_motion(true).unwrap();
    node.animate(
        StyleProperty::Opacity,
        Keyframes::from_values(0.0_f32, 0.5_f32),
        linear(1000.0),
    )
    .unwrap();
    document.advance_time(10.0).unwrap();
    assert!(close(node.computed_style().unwrap().opacity, 0.5));
    assert!(!document.is_animating().unwrap());
}

// ── Headless sampling ───────────────────────────────────────────────────

#[test]
fn headless_render_at_samples_any_time_deterministically() {
    let document = document();
    let node = boxed(&document);
    node.animate(
        StyleProperty::Opacity,
        Keyframes::from_values(0.0_f32, 1.0_f32),
        linear(100.0),
    )
    .unwrap();
    document.advance_time(30.0).unwrap();
    let first = document.render_to_bitmap().unwrap();
    document.advance_time(70.0).unwrap();
    document.advance_time(30.0).unwrap(); // the clock may go back
    assert_eq!(document.render_to_bitmap().unwrap(), first);
}
