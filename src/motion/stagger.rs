//! motion-dom 14.14.0 `stagger(duration, { startDelay, from, ease, grid })`
//! (`utils/stagger.mjs`): the delay, in seconds, of element `i` of `total`.

use super::easing::Ease;
use super::js::hypot;

/// Where the stagger starts from.
#[derive(Debug, Clone, PartialEq)]
pub enum StaggerFrom {
    /// An element index (`from: 2`).
    Index(f64),
    First,
    Last,
    /// `"center"`: any string other than first, last and edges.
    Center,
    Edges,
}

/// `stagger(duration, options)`. `from: "random"` is not ported: it draws
/// from `Math.random`, so no run is reproducible.
#[derive(Debug, Clone)]
pub struct Stagger {
    /// Seconds between neighbours (default 0.1).
    pub duration: f64,
    pub start_delay: f64,
    pub from: StaggerFrom,
    pub ease: Option<Ease>,
    /// `[columns, rows]`.
    pub grid: Option<[f64; 2]>,
}

impl Default for Stagger {
    fn default() -> Self {
        Self {
            duration: 0.1,
            start_delay: 0.0,
            from: StaggerFrom::Index(0.0),
            ease: None,
            grid: None,
        }
    }
}

impl Stagger {
    /// The distances from the origin of every element, then the value that
    /// normalises easing (the largest distance on a grid, else `total`).
    fn distances(&self, total: usize) -> Vec<f64> {
        let [columns, rows] = self.grid.unwrap_or([f64::INFINITY, 1.0]);
        let total_f = total as f64;
        let last_x = columns.min(total_f) - 1.0;
        let last_y = rows.max((total_f / columns).ceil()) - 1.0;
        let scale = match self.from {
            StaggerFrom::First => 0.0,
            StaggerFrom::Last => 1.0,
            _ => 0.5,
        };
        let (origin_x, origin_y) = match self.from {
            StaggerFrom::Index(from) => (from % columns, (from / columns).floor()),
            _ => (last_x * scale, last_y * scale),
        };
        // `lastX ? edgeX : edgeY`: zero and NaN are falsy.
        let truthy = |v: f64| v != 0.0 && !v.is_nan();
        let mut distances: Vec<f64> = (0..total)
            .map(|j| {
                let j = j as f64;
                let x = j % columns;
                let y = (j / columns).floor();
                if self.from == StaggerFrom::Edges {
                    let edge_x = x.min(last_x - x);
                    let edge_y = y.min(last_y - y);
                    let a = if truthy(last_x) { edge_x } else { edge_y };
                    let b = if truthy(last_y) { edge_y } else { edge_x };
                    a.min(b)
                } else {
                    hypot(&[x - origin_x, y - origin_y])
                }
            })
            .collect();
        let normaliser = if self.grid.is_some() {
            distances.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        } else {
            total_f
        };
        distances.push(normaliser);
        distances
    }

    /// The delay in seconds of element `index` of `total`.
    pub fn delay(&self, index: usize, total: usize) -> f64 {
        let distances = self.distances(total);
        let mut delay = self.duration * distances[index];
        let max_delay = self.duration * distances[total];
        if let Some(ease) = &self.ease {
            if max_delay != 0.0 && !max_delay.is_nan() {
                delay = ease.function()(delay / max_delay) * max_delay;
            }
        }
        self.start_delay + delay
    }
}
