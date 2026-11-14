//! Writing a spring's value to an element's inline style, as
//! `@react-spring/web`'s `animated` components do on each change
//! (`applyAnimatedValues` sets `element.style`).

use div::prelude::*;
use div::typed_style::{TransformList, TransformOperation};

/// The style a spring drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleTarget {
    /// `style.opacity = value`.
    Opacity,
    /// `style.transform = translate(<value>px, 0px)`.
    TranslateX,
}

impl StyleTarget {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "opacity" => Some(Self::Opacity),
            "x" => Some(Self::TranslateX),
            _ => None,
        }
    }

    /// Set the inline style on `element`.
    pub fn write(self, element: &Element, value: f64) -> Result<(), String> {
        match self {
            Self::Opacity => element.set_property(StyleProperty::Opacity, (value as f32).into()),
            Self::TranslateX => element.set_property(
                StyleProperty::Transform,
                TransformList(vec![TransformOperation::Translate(
                    LengthValue::px(value as f32),
                    LengthValue::px(0.0),
                )])
                .into(),
            ),
        }
        .map_err(|error| format!("{error:?}"))
    }
}
