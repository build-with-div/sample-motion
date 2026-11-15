# anime.js port (Phase 2, M5)

M5 was planned as a GSAP port. GSAP's licence does not allow one (see
[GSAP's licence](#gsaps-licence)), so M5 ports anime.js 4.5.0 (MIT), which
covers the same tween, timeline, ease and stagger capability.

`src/anime/` ports anime.js's JavaScript engine: `animate()`,
`createTimeline()`, `stagger()` and the eases, for DOM targets' `opacity`
and transform properties. The package's ES module bundle is vendored
unmodified in [fidelity/web/animejs-4.5.0](../fidelity/web/animejs-4.5.0).
The port keeps anime.js's formulas and order of operations:

- the engine's clock and its frame-rate tolerance;
- each timer's iteration, alternation and delay arithmetic, including the
  `~~` truncation and the 12-digit rounding of times;
- the render gate that decides which tween writes on a frame;
- replace composition, which trims, overrides and cancels earlier
  animations of the same property;
- four-decimal rounding of written values, and the transform string
  anime.js builds.

| Module | anime.js source |
| --- | --- |
| `engine.rs` | `engine/engine.js` (`update`, `wake`, `tickEngine`), `core/clock.js`, `timer/timer.js` (constructor, `init`, `reset`, `resume`, `pause`, `cancel`, `resetTime`, `reviveTimer`), `core/render.js` (`render`, `tick`) |
| `animation.rs` | `animation/animation.js` (`JSAnimation`, `generateKeyframes`), `animation/composition.js` (`composeTween`, `overrideTween`, `removeTweenSliblings`), `core/values.js` (`getOriginalAnimatableValue`, `getFunctionValue`), `core/units.js` (angles) |
| `timeline.rs` | `timeline/timeline.js` (`Timeline`, `add`, `set`, `label`, `addTlChild`), `timeline/position.js` |
| `ease.rs` | `easings/eases/parser.js`, `cubicBezier`, `steps`, `linear` |
| `stagger.rs` | `utils/stagger.js` |
| `value.rs` | `core/values.js` (`decomposeRawValue`), `core/transforms.js` (`parseInlineTransforms`, `buildTransformString`), `core/helpers.js` (`round`, `lerp`, `clamp`), JavaScript's `+string` |
| `dom.rs` | the style writes on Div elements (`DivHost`) |
| `frame.rs` | the R1 scaffold: headless frames |

`Runtime` holds what anime.js keeps in module state: the engine, timers,
tweens, the per-target transform cache, the composition lookup and the id
counters. A `StyleHost` receives the inline style writes and answers
computed-style reads. On Div, `DivHost` parses each string anime.js
writes with Div's CSS value parser (`typed_style::parse_literal`) and sets
the inline style, as assigning `el.style.transform` does in a browser.

## What it covers

- **Targets and properties.** Elements, with `opacity` and the transform
  properties (`translateX`/`x`, `translateY`/`y`, `rotate`, `scale`, and
  the rest of `validTransforms`). Values can be numbers, unit strings,
  relative strings (`'+=40'`, `'-=30'`, `'*=2'`) or angles in `deg`,
  `rad` or `turn`.
- **Property values.** `[from, to]`, `{from, to}`, value keyframes
  `[a, b, c]`, object keyframes with per-keyframe `duration`, `delay` and
  `ease`, and the `keyframes` parameter in its array and percentage forms.
- **Timing.** `duration`, `delay`, `loop` (finite and infinite),
  `loopDelay`, `alternate`, `reversed`, `playbackRate`, `playbackEase`,
  and `composition` `'replace'` and `'none'`.
- **Eases.** `in`, `out`, `inOut` and `outIn` with a power, and with
  Quad, Cubic, Quart, Quint, Sine, Circ, Expo, Bounce, Back and Elastic.
  Also `cubicBezier`, `steps` and `linear`.
- **`stagger()`.** Values, ranges and units, `from` (first, center, last,
  an index), `reversed`, `ease`, `start`, and 2D `grid` with `axis`.
- **Timelines.** `createTimeline` with `defaults`, `loop`, `alternate`,
  `delay` and `playbackEase`. `add` with positions (numbers, `'<'`,
  `'<<'`, relative operators, labels, `'label+=n'`, a stagger), plus `set`
  and `label`. A child starts from the value its previous sibling in the
  same timeline leaves.

Not ported (they return `Unsupported`):

- colours, complex strings, CSS variables, attributes, plain objects;
- CSS properties other than `opacity`, and unit conversions that need
  layout (`px` to `%`);
- blend composition, spring and irregular eases;
- callbacks and promises (`onUpdate`, `then`, ...), `seek`, `reverse`,
  `alternate()`, `stretch`, `revert`;
- timers without targets, `tl.call`, `tl.sync`;
- random and jittered staggers, `stagger` `use`, and the automatic grid,
  which reads element positions;
- `waapi.animate()`, scroll observers, draggables, layouts, text
  splitting and SVG helpers.

`Math.pow`, `Math.sin`, `Math.cos` and `Math.asin` are Rust's `f64`
functions, not V8's. They feed eases whose results anime.js rounds to four
decimals before writing. In the cases below every written string matches,
but this is not shown for other inputs.

## The scaffolds

**R1 `requestAnimationFrame`.** anime.js's engine calls
`requestAnimationFrame(tickEngine)` and reads time with `Date.now()`.
`Runtime::animation_frame(now)` runs the queued `tickEngine` with
`Date.now()` at `now`. `frame::headless_frame` advances the document
clock, then runs the frame at the clock's time in whole milliseconds. The
tests drive frames this way. No desktop example was written for M5; the
M4 desktop path (`PlatformEvent::Presented` plus `keep_alive`) would
serve.

**F19 CSSOM inline style serialization.** When an animation starts from a
property's current value, anime.js reads `el.style.transform` or
`el.style.opacity` back, and Chromium returns its CSSOM serialization of
the declaration. Div has no CSSOM string read of an inline style. The port
therefore reads back the string it wrote (`Target::inline`), and
`DivHost::computed_value` formats Div's typed computed opacity with Rust's
`f32` formatting. Chromium serializes numbers to six significant digits,
so the two differ when a value read back has more digits:

- In `gap-f19-readback`, an `x: 300` animation is retargeted at frame 20.
  Chromium reads `translateX(171.292px)`, the port reads its own
  `translateX(171.2925px)`, and the new animation starts from a different
  value.
- 22 of its 51 frames differ. `tests/anime_values.rs` requires them to
  differ, so the case reports when the gap closes.
- The other retargeting cases read back values with six or fewer
  significant digits and match.

Both scaffolds are marked `div-gap:` and listed in
[framework-gaps.md](framework-gaps.md).

## Fidelity

Two comparisons against anime.js in pinned Chromium (`chrome-linux64`
147.0.7727.50, Div's pin), both produced by
[`tools/anime_oracle.py`](../tools/anime_oracle.py). The scenarios are in
[tests/anime/cases.json](../tests/anime/cases.json), shared by both
sides.

**How the page runs anime.js.**

- **Loading.** The page loads the vendored bundle over HTTP from
  127.0.0.1.
- **Time.** The rAF pump of
  [q13-chromium-capture.md](q13-chromium-capture.md), extended to
  `Date.now`. anime.js reads `Date.now()`, not the frame timestamp, and
  starts its engine clock when the module loads. `requestAnimationFrame`,
  `performance.now` and `Date.now` are replaced before it loads, at time
  0. Frame k runs the queued callbacks with `Date.now()` at
  `floor(k * 1000 / 60)`. Then the case's calls for frame k run.
- **Recording.** Chromium resolves `el.style.transform` through a named
  property interceptor, not a prototype accessor. The page therefore
  defines an own accessor on each box's style object. It records the
  value and calls CSSOM `setProperty`, which is what the camel-case
  setter does. The getter is `getPropertyValue`, so reads still return
  Chromium's serialization.
- **Isolation.** Each case gets a fresh page, because the engine is
  module state.

### Writes: 46 cases, 2670 frames, 3737 writes exact

47 cases:

- each property kind, from-to and from values, relative values, angle
  units;
- value, object, array and percentage keyframes;
- delay, loops with alternation and loop delay, infinite loops, reversed,
  playback rate and playback ease, an animation started at frame 17;
- 18 string eases plus the default `out(2)`, `cubicBezier`, `steps` (both
  directions) and `linear` with a stop position;
- staggered delays, values, ranges, units with an ease, a grid, reversed
  with a start;
- replace composition: a retarget, a partial retarget, retargeting one
  transform of several, a delayed overlap, a looped animation replaced;
  composition `'none'`;
- timelines: a sequence with a relative position, labels and `'<'`/`'<<'`
  positions, stagger positions with a relative start, loop and
  alternate, `set`, timeline delay with playback ease, chained values,
  and a timeline cancelled by an `animate()` call;
- `gap-f19-readback` (above).

The oracle records every `el.style.opacity` and `el.style.transform`
assignment, in order, for each frame. `cargo test --test anime_values`
runs the same cases on the port, on Div elements, and requires the same
assignments with the same strings. Every string must also be accepted by
Div's parser.

```sh
python3 tools/anime_oracle.py values DIV_CHECKOUT CHROME_DIR   # WSL
cargo test --test anime_values
```

Result, 14 November 2026:

- **The 46 cases without a gap match in all 2670 frames (3737 writes).**
  This was the port's first comparison run.
- **`gap-f19-readback` differs in 22 of 51 frames**, as described above.
- **Recording the oracle a second time gave a byte-identical file.**

### Frames: 66 of 66 exact

11 cases at frames 0, 8, 16, 24, 36 and 48: opacity, a transform group,
from-to arrays, percentage keyframes, a loop with alternation, parametric
eases, a stagger grid, a retarget, a timeline sequence, timeline stagger
positions and `tl.set`.

The page is 200x120 at device scale 1 (the capture harness's size), with
four 30 px boxes.

- **Chromium:** the pump as above, on the main thread
  (`--disable-threaded-animation`). Each capture follows two of
  Chromium's own frames.
- **Div:** the port with `frame::headless_frame`, then
  `render_to_png_buffer()`. The test checks that the clock reads back the
  exact frame time.

```sh
cargo test --test anime_pixels -- --ignored               # Div frames (Windows)
python3 tools/anime_oracle.py pixels DIV_CHECKOUT CHROME_DIR   # Chromium, WSL
```

Result, 14 November 2026, with Div frames from Windows (`motion-env`, the
CI Skia flags) and Chromium in WSL: two runs, **66 of 66 frames exact in
each**.

These results cover only these cases and frames. They do not cover the
unported features above, other inputs to the eases, or Div's frame
scheduling, which does not exist yet (R1).

## Gaps found

- **F19 CSSOM inline style serialization** (new). Reading
  `el.style.<property>` and `getComputedStyle().getPropertyValue()` as
  CSSOM strings. Owner: engine (style) with a framework API. Size: M.
  Scaffolded as above; measured by `gap-f19-readback`.
- **F20 transform functions in Div's parser** (new, not measured here).
  anime.js writes `translateZ`, `rotateX`/`rotateY`/`rotateZ`, `skew`,
  `skewX`/`skewY`, `perspective`, `translate3d` and `scale3d` when those
  properties are animated. Div's transform parser accepts only
  translate, scale, rotate and matrix
  (`crates/div-style/src/transform_literal.rs:3-7`, whose tests reject
  `rotateZ(1deg)` and `skew(1deg)`). `DivHost` reports such writes as
  errors. Owner: engine (style, paint). Size: M. No case animates them.

## GSAP's licence

Checked on 14 November 2026 for GSAP 3.15.1, the latest on npm.

- The package has no licence file. Its `package.json` and source headers
  point to the "Standard 'no charge' license" at
  <https://gsap.com/standard-license> (Webflow; effective 30 April 2025,
  modified 30 May 2025).
- That licence grants use, reproduction, display and implementation of
  GSAP only for its Permitted Uses (websites, web apps and digital
  interfaces). It grants no right to modify, port or make derivative
  works, and none to sublicense or redistribute. All intellectual property
  stays with Webflow.
- It also prohibits reverse engineering to build Competitive Products.

So this repository does not port GSAP, does not derive code from it, does
not read its source to write its own, does not commit its files and does
not use it as a test oracle. Only GSAP's licence header and `package.json`
were read.

anime.js 4.5.0 is MIT (`LICENSE.md`, "Copyright (c) 2025 Julian Garnier";
the bundle header says `@license MIT`). Its exports cover the GSAP areas
in [design.md](design.md#packages-and-the-platform-features-they-use) as
follows. This is from the export list, not from use, and only the core is
ported here:

| GSAP | anime.js 4.5.0 |
| --- | --- |
| tweens, timelines, eases, stagger | `animate`, `createTimeline`, the eases, `stagger` (ported here) |
| ScrollTrigger | `onScroll` (`ScrollObserver`) |
| Flip | `createLayout` (`AutoLayout`) |
| Draggable | `createDraggable` |
| DrawSVG, MorphSVG, MotionPath | `createDrawable`, `morphTo`, `createMotionPath` |
| SplitText | `splitText` (`TextSplitter`) |
| `gsap.matchMedia` | `createScope` with media queries |

No GSAP area in that table lacks an MIT counterpart, so no gap is listed
for it. GSAP's other plugins (for example CustomEase, ScrollSmoother,
Inertia) were not compared.
