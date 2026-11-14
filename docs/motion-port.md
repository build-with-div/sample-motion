# Motion port (Phase 2, M3 and M3b)

`src/motion/` ports the part of Motion's `animate()` that runs on the Web
Animations API: Motion `14.14.0` (motion-dom 14.14.0, motion-utils 14.7.0),
vendored unmodified in [fidelity/web/motion-14.14.0](../fidelity/web/motion-14.14.0).
For `opacity` and `transform`, Motion turns a transition into one
`element.animate(keyframes, options)` call. The port reproduces that call
with Motion's own formulas and operation order, then makes it on Div.

| Module | Motion source |
| --- | --- |
| `native.rs` | `animateMotionValue`, `getDefaultTransition`, `AsyncMotionValueAnimation`, `supportsBrowserAnimation`, `NativeAnimationExtended`, `NativeAnimation`, `startWaapiAnimation` |
| `spring.rs` | `spring()`, `findSpring`, `getSpringOptions` and the under-, critically and over-damped curves |
| `generator.rs` | `calcGeneratorDuration`, `createGeneratorEasing` (`applyToOptions`) |
| `easing.rs` | the named easings, Motion's cubic-bezier solver, `mapEasingToNativeEasing`, `generateLinearEasing` |
| `stagger.rs` | `stagger()` (motion-dom `utils/stagger.mjs`) |
| `sequence.rs` | `createAnimationsFromSequence` with `calcNextTime`, `calcLabelTime`, `addKeyframes`, `compareByTime`, `defaultOffset`/`fillOffset`, and the keyframe resolver's wildcard fill (framer-motion 14.14.0 `animation/sequence/`) |
| `js.rs` | the JavaScript number semantics the outputs depend on: `Math.round`, `Number` to string, `parseFloat`, V8's `Math.hypot`, and V8's `Math.exp` (used by the react-spring port) |
| `mod.rs` | `resolve` (the call Motion makes), `animate`, `animate_staggered` and `animate_sequence` (making the calls on Div) |

## What Div provides and what is scaffolded

On Div, the call becomes `Element::animate(property, Keyframes, AnimationOptions)`.
Delay, duration, iterations, direction, `fill: both`, per-keyframe offsets and
easings all come from the engine's keyframe effects. Easings use the engine's
`cubic-bezier()`. Motion's spring and long-tail easings use `linear()` with
Blink's fill of the missing inputs.

Gaps (plan IDs as in [framework-gaps.md](framework-gaps.md)):

- **A3, Web Animations API.** One `Element::animate` per property, returning
  an `AnimationId`, stands in for the `Animation` that `element.animate()`
  returns. Without `finished`/`onfinish`, Motion's finish handler (commit
  the final style, then cancel) does not run. The filling animation stays
  instead, and it renders the same final value. A single keyframe (an
  implicit from-value taken from the underlying style) cannot be
  expressed, so the port returns an error. One scaffold, marked
  `div-gap: A3`.
- **R1, `requestAnimationFrame`.** Motion runs some values on its own
  frame loop: `type: "inertia"`, `repeatDelay`, `repeatType: "mirror"`,
  `damping: 0`, and every value other than opacity, transform, filter and
  clip-path. The port reports these as `NotNative::MainThread` and does
  not scaffold them; they wait for R1 (Tier 0).

Not ported:

- **`filter` and `clipPath`.** Motion also runs these natively, but Div has
  only the `filter-blur`, `filter-grayscale` and `clip-path-inset` longhands.
  It has no CSS `filter` function list and no `clip-path` shapes to animate.
  This is the existing A4 gap (F6: "`filter` lists ... `clip-path`",
  engine, M each).
- **`stagger({ from: "random" })`.** It draws from `Math.random`, so no run
  is reproducible.
- **The inertia generator.** It runs on the main thread and waits for R1.

## Fidelity

Two comparisons against Motion running in pinned Chromium
(`chrome-linux64` 147.0.7727.50, Div's pin), both produced by
`tools/motion_oracle.py`.

### Calls: exact on all 52 cases

`tests/motion/cases.json` holds 52 cases:

- the default transitions;
- every named easing, plus bezier, per-segment easings with `times`;
- repeat with reverse and Infinity;
- instant transitions and unchanged keyframes;
- the three main-thread fallbacks;
- 24 spring configurations (physics, duration and bounce, visual
  duration, clamped, delayed, three keyframes, repeated).

`motion_oracle.py values` runs each case through `Motion.animate` with
`Element.prototype.animate` wrapped. It records the arguments in
`tests/motion/motion-14.14.0-oracle.json`: 48 calls, plus 4 cases where
Motion makes none.

`cargo test --test motion_native` compares the port with that file. It
requires every field to be equal exactly, including every point of each
`linear()` string, and it runs in CI on Linux and Windows.

Result on Windows, 13 November 2026: all 52 cases equal, 48 native calls.

### Frames: 90 of 90 exact

Ten cases (opacity, ease-in-out, four springs, a bezier and a back easing
that overshoot, per-segment easings) at
t = 0, 50, 100, 200, 300, 450, 600, 800, 1000 ms, in a 200×120 viewport at
device scale 1.

- **Chromium:** Motion starts the animation, the page waits two frames,
  then every animation is paused and seeked to t, and the capture follows
  two more frames. This is the method M2 measured as byte-stable
  ([q13-chromium-capture.md](q13-chromium-capture.md)), run on the main
  thread since M3b ([capture method](#capture-method)).
- **Div:** the port starts the same animation, then `advance_time(t)` and
  `render_to_png_buffer()`.

```sh
cargo test --test motion_pixels -- --ignored     # Div frames (Windows)
python3 tools/motion_oracle.py pixels DIV_CHECKOUT CHROME_DIR   # Chromium, WSL
```

Result, 13 November 2026, with Div frames from Windows (`motion-env`, the
CI Skia flags) and Chromium in WSL: 90 of 90 frames byte-identical. It was
measured first with threaded animation and no settle frames, then twice
more with the M3b capture method.

The first run had 87 of 90. The three differing frames (the bounce-0.5
spring at 300 and 450 ms, the overshooting bezier at 450 ms) were all
the same 200×15 band. The overshooting transform pushed the box past the
viewport's right edge, and Div drew a horizontal viewport scrollbar. The
Chromium page sets `overflow: hidden` on `html, body`, while the Div side
did not. That was a harness mismatch, not a Div difference. The Div side
now sets `overflow: hidden` on its root too.

These results cover only these cases and times. They do not establish
fidelity for other values, easings or the frame loop.

## M3b: `stagger()` and sequences

`animate(elements, keyframes, { delay: stagger(...) })` gives each element
the delay `stagger(index, total)`. `animate(sequence, options)` resolves a
timeline into one animation per element and value. Each animation has:

- the timeline's total duration;
- the keyframes at their offsets, with wildcards (`null`) where a value
  holds;
- one easing per keyframe.

Each then takes the single-value path above. On Div the calls go through
the same `div-gap: A3` scaffold; M3b adds no new scaffold.

What the sequence port covers:

- **Timing:** `at` as seconds, `"+n"`, `"-n"`, `"<"`, `"<n"`, labels and
  `"label±n"`; nested sequences.
- **Values:** several values and several elements per segment; a stagger
  inside a segment.
- **Springs:** inside a segment, generated over the keyframe delta.
- **Defaults:** `defaultTransition`, including a default spring.
- **Repeats:** segment `repeat` with loop, reverse and mirror, and
  `repeatDelay`. Motion turns these into keyframes, so they run natively.
- **Sequence level:** `repeat` and `delay`.
- **Overlap:** a later segment erases what an earlier one put between its
  own start and end.
- **Leading wildcards:** opacity reads the computed opacity. A transform
  over `transform: none` takes the target's shape with every number zero,
  as Motion does.

A leading wildcard over an existing transform is not ported, because
Motion would read the computed matrix string; the port returns an error.

### Calls: exact on all 37 cases

`tests/motion/sequences.json` holds 12 stagger cases and 25 sequence
cases:

- **Stagger:** `from` first, last, center, an index and edges; `startDelay`;
  a named and a Bézier ease; three grids; a spring; a transform.
- **Sequence:** the features listed above.

`motion_oracle.py sequences` creates the elements, runs Motion and records
every `element.animate()` call, with its element's index, in
`tests/motion/motion-14.14.0-sequences-oracle.json`. That is 111 calls,
plus one case with none (unchanged keyframes).

`cargo test --test motion_sequence` resolves the same cases with the port.
Every call must match the recorded call for that element and property,
field by field, exactly, and every recorded call must be matched.
Keyframe values are checked as well; transform keyframes are compared by
value.

Result on Windows, 13 November 2026: all 37 cases equal, 111 native calls.

Exact comparison needs serde_json's `float_roundtrip` feature. Without it,
6 offsets and 1 delay parsed from the oracle were 1 ulp off, though the
port's values equal what Motion printed.

### Frames: 128 of 128 exact

Sixteen cases at t = 0, 100, 250, 400, 600, 800, 1000 and 1300 ms:

- **Stagger:** default, a grid, a spring, a transform.
- **Sequence:** two values, a wildcard hold, labels, a stagger, springs,
  per-segment easings with `times`, an overshooting Bézier, a reversed
  repeat, a repeat delay, an overlap, a nested sequence, a leading
  wildcard.

The page holds up to nine 20 px boxes at `box_position`, in the same
200×120 viewport.

```sh
cargo test --test motion_pixels -- --ignored            # Div frames (Windows)
python3 tools/motion_oracle.py sequence-pixels DIV_CHECKOUT CHROME_DIR
```

Result, 13 November 2026, with Div frames from Windows and Chromium in
WSL: 128 of 128 frames byte-identical, in two runs out of two.

### Capture method

The first sequence comparison gave 120 of 128. The eight differing frames
were all at t = 0 or 100 ms:

- **Where:** whole 20 px boxes whose animation was in its delay or
  holding its first value.
- **Chromium:** a faint opacity (232–246 instead of 255).
- **Div:** 255, the keyframe value.

Three capture variants were tried on `stagger-default`, box 1:

| Capture | Box 1 at t = 0 and 100 | Result |
| --- | --- | --- |
| Seek, capture at once (M2's method) | 241 in this run, 246 in the first | Differs between runs |
| Seek, wait two frames, capture | 255 | Matched here, but the full comparison still differed in 4, then 2, frames over two runs |
| `--disable-threaded-animation`, seek, wait two frames | 255 | 128 of 128 frames, two runs out of two |

The stale value was the opacity the compositor held when the animation
was paused in real time. A seek to a value equal to the main thread's
previous value did not replace it.

The Chromium side now runs every animation on the main thread and waits
two frames after each seek. With this method, M3's 90 frames are still
exact (two runs). The difference came from the capture, not from Div. It
is also recorded in
[q13-chromium-capture.md](q13-chromium-capture.md).

These results cover only these cases and times.
