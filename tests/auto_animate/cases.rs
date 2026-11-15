//! Reads tests/auto_animate/cases.json, the scenarios shared with
//! `tools/auto_animate_oracle.py`, and runs them on the port.

use div::prelude::*;
use div::typed_style::parse_literal;
use sample_motion::auto_animate::{AutoAnimate, Config, Controller, Event, KeyValue};
use serde_json::{json, Value as Json};

pub fn spec() -> Json {
    serde_json::from_str(include_str!("cases.json")).unwrap()
}

/// `k * 1000 / 60`, the oracle's clock at frame k.
pub fn time(frame: usize) -> f64 {
    frame as f64 * 1000.0 / 60.0
}

/// Set `property: value; ...` on `element`, each value read by Div's CSS
/// value parser (as `style!` does).
fn apply(element: &Element, css: &str) {
    for declaration in css.split(';').map(str::trim).filter(|d| !d.is_empty()) {
        let (name, value) = declaration.split_once(':').unwrap();
        let (name, value) = (name.trim(), value.trim());
        let property =
            StyleProperty::from_css_name(name).unwrap_or_else(|| panic!("no Div property {name}"));
        let parsed = parse_literal(property, value)
            .unwrap_or_else(|error| panic!("{name}: {value}: {error:?}"));
        element.set_property(property, parsed).unwrap();
    }
}

/// A JSON number as `JSON.stringify` writes it: integers without a
/// fraction, `-0` as `0`.
fn num(x: f64) -> Json {
    if x.fract() == 0.0 && x.abs() < 9.0e15 {
        json!(x as i64)
    } else {
        json!(x)
    }
}

/// One case on the port: the oracle's page on a Div document.
pub struct Run {
    pub name: String,
    pub document: Document,
    pub aa: AutoAnimate,
    parent: Element,
    items: Vec<Option<Element>>,
    controller: Option<Controller>,
    item_css: String,
    colors: Vec<String>,
    case: Json,
}

impl Run {
    /// The page, the initial items, and frame 0's calls.
    pub fn new(spec: &Json, case: &Json) -> (Self, Vec<Json>) {
        let document = Document::with_viewport_metrics(
            ViewportMetrics::from_logical_size(200.0, 120.0, 1.0).unwrap(),
        )
        .unwrap();
        apply(
            &document.body(),
            "margin: 0; background-color: white; overflow: hidden",
        );
        let page = &spec["pages"][case["page"].as_str().unwrap()];
        let parent = Element::create(&document, "div").unwrap();
        apply(&parent, page["parent"].as_str().unwrap());
        document.body().append_child(&parent).unwrap();
        if case["reducedMotion"].as_bool().unwrap_or(false) {
            document.set_prefers_reduced_motion(true).unwrap();
        }
        let mut aa = AutoAnimate::new(&document, (200.0, 120.0));
        aa.set_random(|| 0.5);
        let mut run = Self {
            name: case["name"].as_str().unwrap().to_string(),
            document,
            aa,
            parent,
            items: Vec::new(),
            controller: None,
            item_css: page["item"].as_str().unwrap().to_string(),
            colors: spec["colors"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_str().unwrap().to_string())
                .collect(),
            case: case.clone(),
        };
        let parent = run.parent.clone();
        for k in 0..case["items"].as_u64().unwrap() as usize {
            let item = run.item(k);
            run.aa.append_child(&parent, &item).unwrap();
        }
        run.calls(0);
        run.aa.checkpoint();
        let events = run.take();
        (run, events)
    }

    pub fn frames(&self) -> usize {
        self.case["frames"].as_u64().unwrap() as usize
    }

    /// Frame k: advance Div's clock, run the frame, then the case's calls.
    pub fn frame(&mut self, frame: usize) -> Vec<Json> {
        self.document.advance_time(time(frame)).unwrap();
        self.aa.frame(time(frame));
        self.calls(frame);
        self.aa.checkpoint();
        self.take()
    }

    fn item(&mut self, k: usize) -> Element {
        if self.items.len() <= k {
            self.items.resize(k + 1, None);
        }
        if self.items[k].is_none() {
            let element = Element::create(&self.document, "div").unwrap();
            apply(&element, &self.item_css);
            let color = self.colors[k % self.colors.len()].clone();
            apply(&element, &format!("background-color: {color}"));
            self.items[k] = Some(element);
        }
        self.items[k].clone().unwrap()
    }

    fn calls(&mut self, frame: usize) {
        let Some(calls) = self.case["calls"].get(frame.to_string()).cloned() else {
            return;
        };
        let parent = self.parent.clone();
        for call in calls.as_array().unwrap() {
            let call = call.as_array().unwrap();
            let index = |i: usize| call[i].as_u64().unwrap() as usize;
            match call[0].as_str().unwrap() {
                "auto" => {
                    let mut config = Config::default();
                    if let Some(options) = call.get(1) {
                        if let Some(d) = options.get("duration") {
                            config.duration = d.as_f64().unwrap();
                        }
                        if let Some(e) = options.get("easing") {
                            config.easing = e.as_str().unwrap().into();
                        }
                        if let Some(d) = options.get("disrespectUserMotionPreference") {
                            config.disrespect_user_motion_preference = d.as_bool().unwrap();
                        }
                    }
                    self.controller = Some(self.aa.auto_animate(&parent, config));
                }
                "append" => {
                    let node = self.item(index(1));
                    self.aa.append_child(&parent, &node).unwrap();
                }
                "insert" => {
                    let node = self.item(index(1));
                    let child = self.item(index(2));
                    self.aa.insert_before(&parent, &node, &child).unwrap();
                }
                "remove" => {
                    let node = self.item(index(1));
                    self.aa.remove(&node).unwrap();
                }
                "disable" => self.aa.disable(self.controller.as_ref().unwrap()),
                "enable" => self.aa.enable(self.controller.as_ref().unwrap()),
                "destroy" => self.aa.destroy(self.controller.as_ref().unwrap()),
                other => panic!("{}: unknown call {other}", self.name),
            }
        }
    }

    fn key(&self, element: &Element) -> Json {
        if element.is_same_node(&self.parent).unwrap() {
            return json!("p");
        }
        for (k, item) in self.items.iter().enumerate() {
            if let Some(item) = item {
                if element.is_same_node(item).unwrap() {
                    return json!(k.to_string());
                }
            }
        }
        Json::Null
    }

    /// The events as the oracle records them.
    fn take(&mut self) -> Vec<Json> {
        let log = self.aa.take_log();
        log.into_iter()
            .map(|event| match event {
                Event::Animate {
                    element,
                    keyframes,
                    duration,
                    easing,
                } => {
                    let frames: Vec<Json> = keyframes
                        .iter()
                        .map(|frame| {
                            let mut map = serde_json::Map::new();
                            for (name, value) in frame {
                                let value = match value {
                                    KeyValue::Str(s) => json!(s),
                                    KeyValue::Num(n) => num(*n),
                                };
                                map.insert(name.to_string(), value);
                            }
                            Json::Object(map)
                        })
                        .collect();
                    let options = json!({"duration": num(duration), "easing": easing});
                    json!(["animate", self.key(&element), frames, options])
                }
                Event::Cancel(i) => json!(["cancel", i]),
                Event::Finish(i) => json!(["finish", i]),
                Event::End(i) => json!(["end", i]),
                Event::Style {
                    element,
                    property,
                    value,
                } => json!(["style", self.key(&element), property, value]),
                Event::InsertBefore {
                    parent,
                    node,
                    child,
                } => json!([
                    "insertBefore",
                    self.key(&parent),
                    self.key(&node),
                    self.key(&child)
                ]),
                Event::AppendChild { parent, node } => {
                    json!(["appendChild", self.key(&parent), self.key(&node)])
                }
                Event::Remove(node) => json!(["remove", self.key(&node)]),
            })
            .collect()
    }
}
