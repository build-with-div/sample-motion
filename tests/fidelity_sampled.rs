//! Fidelity probe: Div's side of the sampled-time comparison with Chromium.
//!
//! Writes `out/fidelity/div/<case>-<time>.png`. `tools/chromium_sampled.py`
//! renders the same cases in pinned Chromium (CSS animation, negative delay,
//! paused) and compares. Keep the cases equal to that script's `CASES`.
//!
//! ```text
//! cargo test --test fidelity_sampled -- --ignored
//! ```

use div::prelude::*;
use div::typed_style::{
    AnimationOptions, Easing, FillMode, Keyframes, LinearStop, PercentageLinearStop,
    RendererStyleValue, StepPosition, TransformList, TransformOperation, Visibility,
};
use std::path::PathBuf;

pub const TIMES: [f64; 6] = [0.0, 100.0, 250.0, 500.0, 750.0, 1000.0];

fn translate_rotate(x: f32, degrees: f32) -> TransformList {
    TransformList(vec![
        TransformOperation::Translate(LengthValue::px(x), LengthValue::px(0.0)),
        TransformOperation::Rotate(degrees),
    ])
}

fn scale(s: f32) -> TransformList {
    TransformList(vec![TransformOperation::Scale(s, s)])
}

fn visibility(value: Visibility) -> StyleValue {
    StyleValue::Renderer(RendererStyleValue::Visibility(value))
}

fn options(easing: Easing) -> AnimationOptions {
    AnimationOptions {
        duration_ms: 1000.0,
        fill: FillMode::Both,
        easing,
        ..AnimationOptions::default()
    }
}

fn render(case: &str, time_ms: f64) -> Vec<u8> {
    let document = Document::with_viewport_metrics(
        ViewportMetrics::from_logical_size(200.0, 120.0, 1.0).unwrap(),
    )
    .unwrap();
    document.body().set_background_color(Color::WHITE).unwrap();
    let node = Element::create(&document, "div").unwrap();
    node.apply_style(&style! {
        position: absolute; left: 20px; top: 20px; width: 60px; height: 60px
    })
    .unwrap();
    node.set_background_color(Color::BLUE).unwrap();
    document.body().append_child(&node).unwrap();
    let linear = Easing::Linear;
    match case {
        "opacity" => node.animate(
            StyleProperty::Opacity,
            Keyframes::from_values(0.0_f32, 1.0_f32),
            options(linear),
        ),
        "translate-rotate-ease" => node.animate(
            StyleProperty::Transform,
            Keyframes::from_values(translate_rotate(0.0, 0.0), translate_rotate(100.0, 45.0)),
            options(Easing::cubic_bezier(0.25, 0.1, 0.25, 1.0).unwrap()),
        ),
        "background-color" => node.animate(
            StyleProperty::BackgroundColor,
            Keyframes::from_values(
                Color::from_rgba8(255, 0, 0, 255),
                Color::from_rgba8(0, 0, 255, 255),
            ),
            options(linear),
        ),
        "width-steps" => node.animate(
            StyleProperty::Width,
            Keyframes::from_values(LengthValue::px(20.0), LengthValue::px(120.0)),
            options(Easing::steps(4, StepPosition::JumpEnd).unwrap()),
        ),
        "scale-linear-stops" => node.animate(
            StyleProperty::Transform,
            Keyframes::from_values(scale(1.0), scale(2.0)),
            options(
                Easing::linear_stops(vec![
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
                ])
                .unwrap(),
            ),
        ),
        "scale-linear-stops-percent" => node.animate(
            StyleProperty::Transform,
            Keyframes::from_values(scale(1.0), scale(2.0)),
            options(
                Easing::linear_stops_percent(vec![
                    PercentageLinearStop {
                        input_percent: 0.0,
                        output: 0.0,
                    },
                    PercentageLinearStop {
                        input_percent: 75.0,
                        output: 0.25,
                    },
                    PercentageLinearStop {
                        input_percent: 100.0,
                        output: 1.0,
                    },
                ])
                .unwrap(),
            ),
        ),
        "scale-linear" => node.animate(
            StyleProperty::Transform,
            Keyframes::from_values(scale(1.0), scale(2.0)),
            options(linear),
        ),
        // Not animated: separates fractional-scale raster from animation.
        "scale-static-1.75" => node
            .set_property(StyleProperty::Transform, scale(1.75).into())
            .map(|_| div::AnimationId(0)),
        "visibility" => node.animate(
            StyleProperty::Visibility,
            Keyframes::from_values(
                visibility(Visibility::Hidden),
                visibility(Visibility::Visible),
            ),
            options(linear),
        ),
        other => panic!("unknown case {other}"),
    }
    .unwrap();
    document.advance_time(time_ms).unwrap();
    document.render_to_png_buffer().unwrap()
}

#[test]
#[ignore = "writes Div's side of the Chromium comparison"]
fn write_div_samples() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("out/fidelity/div");
    std::fs::create_dir_all(&out).unwrap();
    for case in [
        "opacity",
        "translate-rotate-ease",
        "background-color",
        "width-steps",
        "scale-linear-stops",
        "scale-linear-stops-percent",
        "scale-linear",
        "scale-static-1.75",
        "visibility",
    ] {
        for time in TIMES {
            std::fs::write(out.join(format!("{case}-{time}.png")), render(case, time)).unwrap();
        }
    }
}
