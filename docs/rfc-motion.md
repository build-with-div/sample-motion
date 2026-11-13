# RFC draft: the web animation platform in Div

- Status: draft from the sample-motion investigation, for the Div tracks to
  adopt, change or reject. Nothing here is scheduled.
- Pinned Div: `565db46a`. Evidence: [design.md](design.md), which holds the
  package → feature map, the measurements and the probes.
- Principle: Div implements the **web platform features** that the motion
  packages (Motion, GSAP, react-spring, AutoAnimate, tailwindcss-animate
  with Radix `Presence`, lottie-web) are built on. It matches Chromium's
  behaviour and pixels and is measured with the comparison harness and WPT.
  Springs, timelines, presence and FLIP stay in the ported packages. Div
  adds no bespoke motion API. Where a Rust surface is better (typed
  keyframes, a typed `Animation` handle, futures instead of promises), the
  web semantics stay underneath it unchanged.
- Owners: **engine** (div-style, div-layout, div-paint, div-engine),
  **framework** (divui, div-platform, div-macros). Sizes: S (days), M (one
  to two weeks), L (more).

## Summary

| # | Feature | Owner | Size | Unblocks |
| --- | --- | --- | --- | --- |
| F4 | `requestAnimationFrame` | framework | S–M | Motion, GSAP, react-spring, lottie |
| F3 | Web Animations API surface | framework + engine | M | Motion, AutoAnimate; base of F1, F2 |
| F5 | Frame production: vsync pacing, frame timestamps, timers, occlusion | framework (platform) | M | all |
| F15 | Compositor-driven frames (staged) | engine + compositor + framework | S–M → L | smoothness for all |
| F1 | CSS Transitions | engine | L | shadcn/Radix, any CSS styling |
| F2 | CSS Animations (`@keyframes`) | engine | L | tailwindcss-animate, Radix, View Transitions |
| F7 | Custom properties | engine | L | tailwindcss-animate, Radix, react-spring |
| F8 | Selectors and cascade (alpha A1) | engine | L | tailwindcss-animate, Radix |
| F9 | Media queries, `matchMedia`, OS reduced motion | engine + framework | M + S | all |
| F6 | Interpolation coverage and raster parity | engine | S–M each | all |
| F11 | Resize, Intersection and Mutation observers | framework + engine | M each | Motion, AutoAnimate, react-spring, GSAP |
| F13 | Scroll-driven animation syntax and timeline objects | engine + framework | M | Motion `scroll()`, scroll effects |
| F14 | View Transitions | engine + compositor | L | page/state transitions |
| F16 | SVG painting and geometry | engine (paint) | L | lottie, GSAP DrawSVG/MorphSVG |
| F17 | Canvas 2D | engine + framework | L | lottie canvas renderer |
| F18 | Retaining removed children (React's contract) | framework | S–M | `AnimatePresence`, Radix `Presence` |
| F12 | Pointer Events | framework | (interactions sample) | gestures |

## F4. `requestAnimationFrame`

HTML's *update the rendering* steps, on Div's frame:

```rust
let handle = document.request_animation_frame(move |now: f64| { /* DOMHighResTimeStamp, ms */ });
document.cancel_animation_frame(handle);
```

- Callbacks run once, in registration order, inside `begin_frame` after
  pending events and **before** animation sampling and style, as in HTML.
  Today the only hook (`PlatformEvent::Presented`) runs after present, one
  frame late.
- `now` is the frame's timestamp, the same value as `document.timeline`'s
  current time for that frame.
- A pending callback makes the window want a frame, as a running animation
  does. Hidden or occluded windows stop calling them (F5).
- Available on every window, including `AppSpec` windows. Headless
  `render_at(t)` runs pending callbacks with `now = t`, so rAF-driven ports
  are as deterministic as engine animations.
- Measured by: WPT `html/webappapis/update-rendering` (ordering subset)
  and by the package comparisons.

## F3. Web Animations API

The engine already implements the timing model, all easings, composition,
fill and direction. It also has the controls, by id. What is missing is the
API shape the packages call:

```rust
let animation: Animation = element.animate(
    keyframes![ { opacity: 0.0, transform: translate_y(px(8.0)) }, { opacity: 1.0, transform: none() } ],
    KeyframeAnimationOptions { duration: 200.0, easing: Easing::EASE_OUT, fill: FillMode::Both, ..Default::default() },
)?;
animation.reverse()?;
animation.update_playback_rate(2.0)?;
animation.finished().await?;      // Err on cancel, like the rejected promise
animation.commit_styles()?;
let running = element.get_animations()?;   // and document.get_animations()
```

- **Effects:** several properties per `KeyframeEffect`; implicit from/to
  keyframes (computed values); per-keyframe `offset`, `easing` and
  `composite`; `iterationComposite`; `getComputedTiming`.
- **`Animation`:** play, pause, reverse, finish, cancel, `currentTime`,
  `startTime`, `playbackRate`, `updatePlaybackRate` (pending playback rate),
  `ready` and `finished` futures, `onfinish`/`oncancel` events,
  `playState`, `pending`, `persist`, `commitStyles`, and **automatic removal
  of replaced filling animations** (`replaceState`, `remove` event). Motion
  and AutoAnimate depend on finished-and-removed semantics to avoid
  unbounded filling animations.
- **Timeline:** `document.timeline` (`DocumentTimeline`); `ScrollTimeline`
  and `ViewTimeline` objects over the engine's existing timelines (F13).
- **Keyword easings:** `ease`, `ease-in`, `ease-out`, `ease-in-out`,
  `step-start`, `step-end` as constants equal to their CSS definitions.
- The existing `Element::animate(property, Keyframes, options) ->
  AnimationId` stays as a thin wrapper or is retired.
- Measured by: WPT `web-animations` (151 files, 133 testharness), via the
  script translation in design §Fidelity.

## F5. Frame production

- Pace frames to the display's refresh rate on every backend:
  - Windows: DXGI waitable swap chain or `DwmFlush`; today the softbuffer
    path has no throttle at all.
  - Wayland: `wl_surface.frame` callbacks (`pre_present_notify`).
  - X11: vsync.
- Measured today: with 100 animated cells, both backends ran CPU-bound at
  64–68 frames/s on a 239 Hz display (design §Measurements).
- Stamp each frame with its expected present time, not the redraw start.
- High-resolution timers for `task::time`: `sleep(1 ms)` takes 15.4 ms at
  the median on Windows (design §Timers).
- Throttle occluded windows as minimized ones already are; rAF stops, and
  the document clock stays on wall time so CSS animations resume in place.

## F15. Compositor-driven frames

The numbers to beat are in design §Measurements:
- One animated 24 px square at 1280 × 800 costs about 7 ms per frame, nearly
  all of it full-viewport raster.
- Style sampling grows ×17 from 100 to 1000 animated elements.
- With 1000 animations running, starting one more takes 16 ms.

Staged so each step pays off alone:

1. **O(1) animation start** and **targeted style sampling**: apply sampled
   values to the animated elements' computed style without re-cascading
   every root. (engine, S–M)
2. **Damage**: union of old and new visual rects instead of full-viewport
   damage (`div-engine/src/lib.rs:2686-2691`); partial raster; present with
   damage (`present_with_damage`, `eglSwapBuffersWithDamage`). (engine +
   framework, M)
3. **Layers**: promote elements with running transform/opacity/filter
   animations or `will-change` and cache their raster. A frame then
   re-composites with new matrices and alpha. Use Chromium's promotion and
   raster-scale rules: design §Fidelity shows animated fractional scales
   already differ at the edges. The existing `RetainedSceneSnapshot` and
   the unused `SceneMailbox` were laid out for this. (engine + compositor,
   L)
4. **Off-thread curves** for promoted layers, so a busy UI thread does not
   stall them. This is Div's existing threading-model release blocker.
   Measured today: a 50 ms block produced a 65 ms frame. (L)

## F1, F2. CSS Transitions and CSS Animations

Through the typed style rules of alpha A1 (RFC 0001 already orders
transitions and animations in the cascade):

- `transition-*` longhands and shorthand. Transitions start on a
  computed-value change during style, with CSS's interruption and reversal
  rules. Add `transitionrun/start/end/cancel`, `transition-behavior:
  allow-discrete` and `@starting-style` (needed for exit and entry without
  JavaScript).
- `@keyframes` and `animation-*`, including `animation-timeline` and
  `animation-range` (F13).
- Events carry `animationName`/`propertyName`, `elapsedTime` and
  `pseudoElement` on the `Event` itself.
- `getComputedStyle().animationName` (Radix `Presence` reads it).
- Measured by: WPT css-transitions (180 files) and css-animations (207),
  through the script translation plus the census's sample-at-time support.

## F6. Interpolation coverage and raster parity

- `visibility`: Div flips it at p = 0.5; Chromium shows the element when
  either end is visible for 0 < p < 1 (pixel-confirmed). (S)
- Raster parity for animated fractional `scale()` (the static scale is
  exact). (S–M, with F15 step 3)
- Individual `translate`, `rotate`, `scale`. `filter` lists.
  `box-shadow`. `clip-path`. `display` with allow-discrete.
  `interpolate-size`/`calc-size()` (height: auto). (M each)
- Measured by: WPT's 327 interpolation-table files (1 137
  `test_interpolation` blocks), generated into value and pixel tests.

## F7, F8, F9. Custom properties, selectors, media queries

These are already on Div's alpha path:
- F7, custom properties: `var()` inside `@keyframes`, `@property`,
  `setProperty('--x')`. tailwindcss-animate's keyframes are written
  entirely in `--tw-enter-*` variables.
- F8, selectors: A1.
- F9, media queries: `prefers-reduced-motion` plus `matchMedia` with change
  events.

The motion-specific part is reading the OS setting:
- Windows: `SPI_GETCLIENTAREAANIMATION`, updated on `WM_SETTINGCHANGE`.
- Linux: the `org.freedesktop.appearance` portal, or GNOME's
  `enable-animations`.

Feed it into the media feature, so `Document::set_prefers_reduced_motion`
becomes an override rather than the only source.

## F11. Observers

- ResizeObserver and IntersectionObserver, delivered at HTML's point in
  *update the rendering*.
- MutationObserver for `childList`/`attributes`. AutoAnimate watches
  `childList`.
- Measured by: WPT `resize-observer`, `intersection-observer`,
  `dom/nodes/MutationObserver-*`.

## F13. Scroll-driven animations

The engine already has scroll and view timelines with named ranges. Expose
them as CSS (`animation-timeline: scroll()/view()`, `scroll-timeline-*`,
`view-timeline-*`, `animation-range`) and as `ScrollTimeline`/
`ViewTimeline` objects for F3. Qualify against WPT `scroll-animations`
(226 files, 75 tentative).

## F14. View Transitions

`startViewTransition(callback)` captures old snapshots, runs the update, and
builds the `::view-transition` pseudo-element tree. Then the CSS animations
run on it (F2), and `view-transition-name` and `view-transition-class` apply.
It needs snapshot layers (F15 step 3). Measured by WPT
`css/css-view-transitions` (445 files, 306 reftests).

## F16, F17. SVG and Canvas 2D

lottie-web renders through SVG (paths with strokes, dashes, masks, clip
paths, gradients, group transforms) or Canvas 2D. Both are large, and are
shared with the charts and canvas samples. This RFC only records that
motion needs them. Lottie also needs `getTotalLength`/`getPointAtLength`
and `pathLength`, as GSAP DrawSVG and Motion's path drawing do.

## F18. Retaining removed children

`AnimatePresence` and Radix `Presence` are React components. They work
because React lets a component keep rendering a child that its parent
removed, until the exit animation's `finished` or `animationend`. divui's
`Show`/`For` unmount at once. First verify whether a component can do this
through `DynChild`. If it cannot, add a renderer contract for retained
keyed children. This is not a web feature; it is the React-equivalent
contract the ports need. (S–M)

## Fidelity

- **Platform:** translate WPT testharness scripts to native calls by
  extending Div's `tools/wpt/javascript_mutation_ir.mjs`. Use Chromium's
  `-expected.txt` baselines as the oracle where Chromium itself fails. Also
  generate tests from the interpolation tables, and teach the census to
  sample at time t. Details and counts are in design §Fidelity.
- **Package:** run the original package in pinned Chromium on driven
  frames and the Rust port on Div's headless clock, from the same scenario,
  and diff exactly.

## Out of scope for Div

- Spring, decay and inertia physics.
- Timeline sequencing.
- FLIP and layout-animation scale correction.
- Gesture recognition.
- Presence components.
- Lottie parsing.

These are the packages' logic and belong in their ports.

## Open questions

1. Is there one F3 API with the CSS-created animations visible in
   `get_animations()` (as on the web), and does the existing
   `Element::animate` stay?
2. Which Chromium frame driver (BeginFrame control or virtual time) gives
   byte-stable captures for compositor-driven animations, for package-level
   comparisons?
3. Does F18 need a renderer change, or can `DynChild` already retain
   children?
