# Motion port (Phase 2, M3)

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
| `js.rs` | the JavaScript number semantics the outputs depend on: `Math.round`, `Number` to string |
| `mod.rs` | `resolve` (the call Motion makes) and `animate` (making it on Div) |

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

Not yet ported: `filter` and `clipPath` (also native in Motion), `stagger`,
sequences, and the inertia generator (main thread, R1).

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
  then every animation is paused and seeked to t. This is the method M2
  measured as byte-stable ([q13-chromium-capture.md](q13-chromium-capture.md)).
- **Div:** the port starts the same animation, then `advance_time(t)` and
  `render_to_png_buffer()`.

```sh
cargo test --test motion_pixels -- --ignored     # Div frames (Windows)
python3 tools/motion_oracle.py pixels DIV_CHECKOUT CHROME_DIR   # Chromium, WSL
```

Result, 13 November 2026, with Div frames from Windows (`motion-env`, the
CI Skia flags) and Chromium in WSL: 90 of 90 frames byte-identical.

The first run had 87 of 90. The three differing frames (the bounce-0.5
spring at 300 and 450 ms, the overshooting bezier at 450 ms) were all
the same 200×15 band. The overshooting transform pushed the box past the
viewport's right edge, and Div drew a horizontal viewport scrollbar. The
Chromium page sets `overflow: hidden` on `html, body`, while the Div side
did not. That was a harness mismatch, not a Div difference. The Div side
now sets `overflow: hidden` on its root too.

These results cover only these cases and times. They do not establish
fidelity for other values, easings or the frame loop.
