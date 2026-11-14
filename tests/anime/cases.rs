//! Reads tests/anime/cases.json, the scenarios shared with
//! `tools/anime_oracle.py`, and runs them on the port.

use div::prelude::*;
use sample_motion::anime::dom::DivHost;
use sample_motion::anime::ease::EaseSpec;
use sample_motion::anime::frame::headless_frame;
use sample_motion::anime::stagger::{From, Stagger};
use sample_motion::anime::{Obj, Raw, Runtime, StyleWrite, TargetId, TimerId, Value};
use serde_json::Value as Json;
use std::rc::Rc;

pub fn spec() -> Json {
    serde_json::from_str(include_str!("cases.json")).unwrap()
}

fn raw(json: &Json) -> Raw {
    match json {
        Json::Number(n) => Raw::Number(n.as_f64().unwrap()),
        Json::String(s) => Raw::String(s.clone()),
        other => panic!("not a number or string: {other}"),
    }
}

fn numbers(json: &Json) -> Vec<f64> {
    json.as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_f64().unwrap())
        .collect()
}

/// A JSON parameter as the JavaScript value the oracle passes to anime.js.
/// Each `{"stagger": ...}` is one `stagger()` call.
pub fn revive(json: &Json) -> Value {
    match json {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => Value::Number(n.as_f64().unwrap()),
        Json::String(s) => Value::String(s.clone()),
        Json::Array(items) => Value::Array(items.iter().map(revive).collect()),
        Json::Object(map) => {
            if let Some(value) = map.get("stagger") {
                return Value::Stagger(Rc::new(stagger(value, map.get("params"))));
            }
            if let Some(args) = map.get("cubicBezier") {
                return Value::Ease(EaseSpec::CubicBezier(numbers(args)));
            }
            if let Some(args) = map.get("steps") {
                let args = args.as_array().unwrap();
                return Value::Ease(EaseSpec::Steps(
                    args.first().map(|n| n.as_f64().unwrap()),
                    args.get(1).is_some_and(|b| b.as_bool().unwrap()),
                ));
            }
            if let Some(args) = map.get("linear") {
                return Value::Ease(EaseSpec::Linear(
                    args.as_array().unwrap().iter().map(raw).collect(),
                ));
            }
            Value::Object(object(json))
        }
    }
}

pub fn object(json: &Json) -> Obj {
    Obj(json
        .as_object()
        .unwrap_or_else(|| panic!("not an object: {json}"))
        .iter()
        .map(|(k, v)| (k.clone(), revive(v)))
        .collect())
}

fn stagger(value: &Json, params: Option<&Json>) -> Stagger {
    let empty = serde_json::Map::new();
    let params = params.map_or(&empty, |p| p.as_object().unwrap());
    let (value, range_end) = match value {
        Json::Array(range) => (raw(&range[0]), Some(raw(&range[1]))),
        other => (raw(other), None),
    };
    let from = match params.get("from") {
        None => From::First,
        Some(Json::String(s)) if s == "first" => From::First,
        Some(Json::String(s)) if s == "center" => From::Center,
        Some(Json::String(s)) if s == "last" => From::Last,
        Some(Json::Number(n)) => From::Index(n.as_f64().unwrap()),
        Some(other) => panic!("stagger from {other}"),
    };
    Stagger::new(
        value,
        range_end,
        from,
        params.get("reversed").is_some_and(|r| r.as_bool().unwrap()),
        params
            .get("ease")
            .map(|e| EaseSpec::Name(e.as_str().unwrap().into())),
        params.get("grid").map(numbers),
        params
            .get("axis")
            .map(|a| a.as_str().unwrap().chars().next().unwrap()),
        params.get("start").map(raw),
    )
}

fn targets(json: &Json) -> Vec<TargetId> {
    json.as_array()
        .unwrap()
        .iter()
        .map(|t| TargetId(t.as_u64().unwrap() as usize))
        .collect()
}

/// The page of tools/anime_oracle.py: a 200x120 white viewport (the
/// capture harness's) and four 30 px blue boxes at (10 + 40 i, 45).
pub fn stage(boxes: usize) -> (Document, Vec<Element>) {
    let document = Document::with_viewport_metrics(
        ViewportMetrics::from_logical_size(200.0, 120.0, 1.0).unwrap(),
    )
    .unwrap();
    document
        .body()
        .apply_style(&style! { overflow: hidden })
        .unwrap();
    document.body().set_background_color(Color::WHITE).unwrap();
    let elements = (0..boxes)
        .map(|i| {
            let node = Element::create(&document, "div").unwrap();
            node.apply_style(&style! {
                position: absolute; top: 45px; width: 30px; height: 30px
            })
            .unwrap();
            node.set_property(
                StyleProperty::Left,
                LengthValue::px(10.0 + 40.0 * i as f32).into(),
            )
            .unwrap();
            node.set_background_color(Color::BLUE).unwrap();
            document.body().append_child(&node).unwrap();
            node
        })
        .collect();
    (document, elements)
}

/// One case running on the port, on a Div page.
pub struct Run {
    pub name: String,
    pub document: Document,
    pub runtime: Runtime,
    timelines: Vec<TimerId>,
    case: Json,
}

/// `floor(k * 1000 / 60)`, the oracle's `Date.now()` at frame k.
pub fn time(frame: usize) -> f64 {
    frame as f64 * 1000.0 / 60.0
}

impl Run {
    /// Load anime.js at time 0 and run frame 0's calls.
    pub fn new(case: &Json, boxes: usize) -> (Self, Vec<StyleWrite>) {
        let (document, elements) = stage(boxes);
        let runtime = Runtime::new(Box::new(DivHost::new(elements)), 0.0);
        let mut run = Self {
            name: case["name"].as_str().unwrap().to_string(),
            document,
            runtime,
            timelines: Vec::new(),
            case: case.clone(),
        };
        run.calls(0);
        let writes = run.runtime.take_writes();
        (run, writes)
    }

    pub fn frames(&self) -> usize {
        self.case["frames"].as_u64().unwrap() as usize
    }

    /// Frame k: the engine's frame callbacks, then the frame's calls.
    pub fn frame(&mut self, frame: usize) -> Vec<StyleWrite> {
        headless_frame(&self.document, &mut self.runtime, time(frame)).unwrap();
        self.calls(frame);
        self.runtime.take_writes()
    }

    fn calls(&mut self, frame: usize) {
        let Some(calls) = self.case["calls"].get(frame.to_string()) else {
            return;
        };
        self.runtime.set_time(time(frame).floor());
        let name = self.name.clone();
        for call in calls.as_array().unwrap() {
            let call = call.as_array().unwrap();
            let position = call.get(4).map(revive);
            let result = match call[0].as_str().unwrap() {
                "animate" => self
                    .runtime
                    .animate(&targets(&call[1]), &object(&call[2]))
                    .map(|_| ()),
                "timeline" => self.runtime.create_timeline(&object(&call[1])).map(|tl| {
                    self.timelines.push(tl);
                }),
                "add" => self.runtime.timeline_add(
                    self.timelines[call[1].as_u64().unwrap() as usize],
                    &targets(&call[2]),
                    &object(&call[3]),
                    position.as_ref(),
                ),
                "set" => self.runtime.timeline_set(
                    self.timelines[call[1].as_u64().unwrap() as usize],
                    &targets(&call[2]),
                    &object(&call[3]),
                    position.as_ref(),
                ),
                "label" => self.runtime.timeline_label(
                    self.timelines[call[1].as_u64().unwrap() as usize],
                    call[2].as_str().unwrap(),
                    call.get(3).map(revive).as_ref(),
                ),
                other => panic!("{name}: unknown call {other}"),
            };
            result.unwrap_or_else(|error| panic!("{name}: {error:?}"));
        }
    }
}
