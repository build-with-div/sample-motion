//! `stagger(value, params)` of `utils/stagger.js`, for index and 2D grid
//! distances. Random order, jitter, `use` and the automatic grid (which
//! reads element positions) are not ported.

use super::ease::{parse_ease, EaseFn, EaseSpec};
use super::value::{round, units_exec, Raw, Unsupported};
use crate::motion::js::{number_to_string, parse_float};
use std::cell::RefCell;

/// Where the stagger counts from (`params.from`).
#[derive(Debug, Clone, PartialEq)]
pub enum From {
    First,
    Center,
    Last,
    Index(f64),
}

/// The parameters of one `stagger()` call. Each call is one function with
/// its own cached distances, as in anime.js.
#[derive(Debug)]
pub struct Stagger {
    /// `val`: one value, or a `[start, end]` range.
    pub value: Raw,
    pub range_end: Option<Raw>,
    pub from: From,
    pub reversed: bool,
    pub ease: Option<EaseSpec>,
    pub grid: Option<Vec<f64>>,
    pub axis: Option<char>,
    /// `params.start`: a number, or a timeline position string.
    pub start: Option<Raw>,
    state: RefCell<State>,
}

#[derive(Debug, Default)]
struct State {
    values: Vec<f64>,
    max_value: f64,
    from_index: f64,
    cached_offset: Option<Raw>,
}

/// The timeline a stagger resolves its `start` against
/// (`parseTimelinePosition(tl, ...)`).
pub trait PositionResolver {
    fn iteration_duration(&self) -> f64;
    fn parse_position(&self, position: &Raw) -> Result<f64, Unsupported>;
}

impl Stagger {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        value: Raw,
        range_end: Option<Raw>,
        from: From,
        reversed: bool,
        ease: Option<EaseSpec>,
        grid: Option<Vec<f64>>,
        axis: Option<char>,
        start: Option<Raw>,
    ) -> Self {
        Self {
            value,
            range_end,
            from,
            reversed,
            ease,
            grid,
            axis,
            start,
            state: RefCell::new(State::default()),
        }
    }

    /// Call the stagger function for target `i` of `total`, with a timeline
    /// when it places timeline children (`tl.add(targets, params,
    /// stagger(...))`). Returns a number, or a string with the value's unit.
    pub fn call(
        &self,
        i: usize,
        total: usize,
        tl: Option<&dyn PositionResolver>,
    ) -> Result<Raw, Unsupported> {
        let parse_number = |raw: &Raw| match raw {
            Raw::Number(n) => *n,
            Raw::String(s) => parse_float(s),
            Raw::Undefined => f64::NAN,
        };
        let is_range = self.range_end.is_some();
        let val1 = parse_number(&self.value);
        let val2 = self.range_end.as_ref().map_or(0.0, parse_number);
        let unit_source = match self.range_end.as_ref().unwrap_or(&self.value) {
            Raw::Number(n) => number_to_string(*n),
            Raw::String(s) => s.clone(),
            Raw::Undefined => "undefined".into(),
        };
        let unit = units_exec(&unit_source).map(|(_, u)| u.to_string());
        // `params.start || 0 + (isRange ? val1 : 0)`
        let start_truthy = match &self.start {
            Some(Raw::Number(n)) => *n != 0.0 && !n.is_nan(),
            Some(Raw::String(s)) => !s.is_empty(),
            _ => false,
        };
        let start = if start_truthy {
            self.start.clone().unwrap()
        } else {
            Raw::Number(if is_range { val1 } else { 0.0 })
        };
        let stagger_ease: Option<EaseFn> = match &self.ease {
            Some(spec) => Some(parse_ease(spec)?),
            None => None,
        };
        let mut state = self.state.borrow_mut();
        if state.values.is_empty() {
            state.from_index = match self.from {
                From::Index(n) => n,
                _ => 0.0,
            };
        }
        let total_f = total as f64;
        match self.from {
            From::Center => state.from_index = (total_f - 1.0) / 2.0,
            From::Last => state.from_index = total_f - 1.0,
            _ => {}
        }
        if state.values.is_empty() {
            let from_index = state.from_index;
            for index in 0..total {
                let index_f = index as f64;
                let value = match &self.grid {
                    None => (from_index - index_f).abs(),
                    Some(grid) => {
                        if grid.len() != 2 {
                            return Err(Unsupported("only 2D stagger grids are ported".into()));
                        }
                        let (from_x, from_y) = match self.from {
                            From::Center => ((grid[0] - 1.0) / 2.0, (grid[1] - 1.0) / 2.0),
                            _ => (
                                js_rem(from_index, grid[0]),
                                js_rem((from_index / grid[0]).floor(), grid[1]),
                            ),
                        };
                        let to_x = js_rem(index_f, grid[0]);
                        let to_y = js_rem((index_f / grid[0]).floor(), grid[1]);
                        let dx = from_x - to_x;
                        let dy = from_y - to_y;
                        let mut value = (dx * dx + dy * dy).sqrt();
                        match self.axis {
                            Some('x') => value = -dx,
                            Some('y') => value = -dy,
                            _ => {}
                        }
                        value
                    }
                };
                state.values.push(value);
            }
            let mut max_value = state.values[0];
            for k in 1..total {
                if state.values[k] > max_value {
                    max_value = state.values[k];
                }
            }
            state.max_value = max_value;
            if stagger_ease.is_some() || self.reversed {
                for k in 0..total {
                    let mut v = state.values[k];
                    if let Some(ease) = &stagger_ease {
                        v = ease(v / max_value) * max_value;
                    }
                    if self.reversed {
                        v = if self.axis.is_some() {
                            -v
                        } else {
                            (max_value - v).abs()
                        };
                    }
                    state.values[k] = v;
                }
            }
        }
        let spacing = if is_range {
            (val2 - val1) / state.max_value
        } else {
            val1
        };
        if state.cached_offset.is_none() {
            state.cached_offset = Some(match tl {
                Some(tl) => {
                    let position = if self.start.is_none() {
                        Raw::Number(tl.iteration_duration())
                    } else {
                        start.clone()
                    };
                    Raw::Number(tl.parse_position(&position)?)
                }
                None => start.clone(),
            });
        }
        let offset = match state.cached_offset.as_ref().unwrap() {
            Raw::Number(n) => *n,
            _ => {
                return Err(Unsupported(
                    "a string stagger start outside a timeline concatenates strings".into(),
                ))
            }
        };
        let step = spacing * round(state.values[i], 2);
        // `(spacing * round(...)) || 0`
        let step = if step == 0.0 || step.is_nan() {
            0.0
        } else {
            step
        };
        let output = offset + step;
        Ok(match unit {
            Some(unit) => Raw::String(format!("{}{unit}", number_to_string(output))),
            None => Raw::Number(output),
        })
    }
}

/// JavaScript `%` for the non-negative grid arithmetic.
fn js_rem(a: f64, b: f64) -> f64 {
    a % b
}
