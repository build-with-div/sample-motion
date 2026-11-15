//! anime.js's style writes and reads on Div elements.

use super::{StyleHost, TargetId};
use div::prelude::*;
use div::typed_style::parse_literal;

/// Div elements as anime.js targets. A write parses the string anime.js
/// wrote with Div's CSS value parser and sets the inline style, as
/// assigning `el.style.opacity` or `el.style.transform` does.
pub struct DivHost {
    pub elements: Vec<Element>,
    /// Writes Div could not parse or set, in order.
    pub errors: Vec<String>,
}

impl DivHost {
    pub fn new(elements: Vec<Element>) -> Self {
        Self {
            elements,
            errors: Vec::new(),
        }
    }
}

impl StyleHost for DivHost {
    fn computed_value(&self, target: TargetId, property: &str) -> String {
        match property {
            // div-gap: F19 CSSOM inline style serialization. Div's computed
            // style is typed; this formats the number with Rust's shortest
            // `f32` form, which is not CSSOM's serialization.
            "opacity" => match self.elements[target.0].computed_style() {
                Ok(style) => format!("{}", style.opacity()),
                Err(_) => String::new(),
            },
            _ => String::new(),
        }
    }

    fn set_style(&mut self, target: TargetId, property: &str, value: &str) {
        let property_id = match property {
            "opacity" => StyleProperty::Opacity,
            "transform" => StyleProperty::Transform,
            other => {
                self.errors.push(format!("no Div property for {other:?}"));
                return;
            }
        };
        let result = parse_literal(property_id, value)
            .map_err(|error| format!("{error:?}"))
            .and_then(|parsed| {
                self.elements[target.0]
                    .set_property(property_id, parsed)
                    .map_err(|error| format!("{error:?}"))
            });
        if let Err(error) = result {
            self.errors.push(format!(
                "{property}: {value:?} on target {}: {error}",
                target.0
            ));
        }
    }

    fn errors(&self) -> &[String] {
        &self.errors
    }
}
