//! A react-spring `SpringValue` on the desktop: a box springs between two
//! positions, retargeted every 700 ms so that most moves interrupt the
//! previous one and carry its velocity.
//!
//! ```text
//! cargo run --example react_spring_toggle [seconds]
//! ```
//!
//! Frames come from `PlatformEvent::Presented` with a keep-alive animation
//! (the R1 scaffolds in `sample_motion::react_spring::frame`). Values are
//! written one frame late, after present; R1 `requestAnimationFrame` is
//! what removes that.

use div::prelude::*;
use sample_motion::react_spring::dom::StyleTarget;
use sample_motion::react_spring::frame::{keep_alive, presented_time};
use sample_motion::react_spring::{preset, Runtime, SpringProps, To};
use std::cell::RefCell;
use std::rc::Rc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let seconds: f64 = std::env::args().nth(1).map_or(Ok(5.0), |v| v.parse())?;
    let mut app = App::builder()
        .title("react-spring toggle")
        .size(LogicalSize::new(480.0, 160.0))
        .build()?;
    let exit = app.exit_handle();
    let runtime = Rc::new(RefCell::new(Runtime::new()));
    let spring = runtime.borrow_mut().create_at(0.0);
    let target: Rc<RefCell<Option<Element>>> = Rc::default();
    let config = Rc::new(preset("wobbly").unwrap());

    {
        let runtime = runtime.clone();
        let target = target.clone();
        let mut next_toggle = 0.0;
        let mut right = false;
        app.on_platform_event(move |event| {
            let Some(time) = presented_time(event) else {
                return;
            };
            let mut runtime = runtime.borrow_mut();
            runtime.animation_frame(time);
            if time >= next_toggle {
                right = !right;
                next_toggle = time + 700.0;
                runtime.set_time(time);
                runtime
                    .start(
                        spring,
                        SpringProps {
                            to: Some(To::Value(if right { 360.0 } else { 0.0 })),
                            config: Some(config.clone()),
                            ..SpringProps::default()
                        },
                    )
                    .unwrap();
                runtime.perform_microtask_checkpoint();
            }
            if let (Some(element), Some(value)) = (target.borrow().as_ref(), runtime.get(spring)) {
                StyleTarget::TranslateX.write(element, value).unwrap();
            }
            if time > seconds * 1000.0 {
                exit.request_exit();
            }
        });
    }

    app.run(move || {
        let document = current_document();
        let root = Element::create(&document, "div").unwrap();
        root.apply_style(&style! { position: relative; width: 480px; height: 160px })
            .unwrap();
        keep_alive(&document, &root).unwrap();
        let node = Element::create(&document, "div").unwrap();
        node.apply_style(&style! {
            position: absolute; left: 20px; top: 50px; width: 60px; height: 60px
        })
        .unwrap();
        node.set_background_color(Color::BLUE).unwrap();
        root.append_child(&node).unwrap();
        *target.borrow_mut() = Some(node);
        ViewNode::Element(root)
    })?;
    Ok(())
}
