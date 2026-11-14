# Sample Motion: design

A dogfood sample for **animation and motion** on
[Div](https://github.com/build-with-div/div). The approach is to **port the
npm packages professionals use for motion and have Div implement the web
platform features that power them**, faithfully and measured against pinned
Chromium. Library logic (springs, timelines, presence, FLIP) belongs to the
ported packages, which live in samples like this one. Div implements the
platform underneath them. Workarounds in this repository are temporary
scaffolding. Each is labelled `// div-gap: F<n>` with the platform feature
that will replace it, and none is proposed as the solution.

Phase 1 (this document) names the packages, maps them to platform features,
and records what Div has, what is missing, who owns it and how big it is.
Phase 2 ports the packages. It started on 13 November 2026; its milestones
are [below](#phase-2-porting-the-packages).

Everything below was checked against Div **`565db46a`** (build-with-div/div
`main`, 13 November 2026). The checks were made by reading the source and
by running the probes in this repository on Windows 11 (24 cores, RTX 3090,
a 5120-px display at 239 Hz). File references are to the Div tree at that
commit. The proposed platform work is in [rfc-motion.md](rfc-motion.md).

## Summary

- **Packages:** Motion (framer-motion), GSAP, react-spring, AutoAnimate,
  tailwindcss-animate (shadcn's enter/exit, with Radix `Presence`) and
  lottie-web.
- **What powers them:**
  - CSS Transitions and `@keyframes` animations
  - the Web Animations API
  - `requestAnimationFrame`
  - geometry reads and the Resize, Intersection and Mutation observers
  - Pointer Events
  - scroll-driven animations
  - View Transitions
  - `prefers-reduced-motion`
  - custom properties and selectors
  - compositor-driven transform and opacity
  - SVG and Canvas 2D for Lottie
- **Div's engine is ahead of its platform surface:**
  - It already has Chromium's animation timing model, every CSS easing
    function, composition, the playback controls, scroll and view
    timelines, and a deterministic clock. In a sampled-time comparison
    with pinned Chromium, 46 of 54 frames were pixel-identical.
  - None of it is reachable the way the packages reach it. There are no
    CSS `transition`/`animation` properties and no `@keyframes`. The
    Web Animations API is a single-property call with id-based controls.
    There is no `requestAnimationFrame`, no observers, no media queries,
    no View Transitions, no stroked SVG and no Canvas 2D.
- **Frames are expensive:**
  - Every animated frame re-cascades all styles, re-records the whole
    paint and rasters the full viewport on the CPU: about 7 ms at
    1280 × 800 for one animated square, measured under load.
  - Nothing paces frames at the display's refresh rate, and timers tick
    at 15.6 ms on Windows.
  - Starting N animations is quadratic.

## Packages and the platform features they use

| Package | What professionals use it for | Platform features it is built on |
| --- | --- | --- |
| **Motion** (`motion`, formerly framer-motion) | Declarative and imperative animation, springs, gestures, layout animations, `AnimatePresence`, scroll effects | **WAAPI** `element.animate` for opacity, transform, filter and clip-path, with springs exported as `linear()` easings (F3). **rAF** frame loop for the other values (F4). Inline style and custom-property writes (F7). **getBoundingClientRect** plus transform/scale correction for layout animations (F10). **ResizeObserver** and **IntersectionObserver** (`inView`) (F11). **Pointer Events** for hover, press and drag (F12). **ScrollTimeline** for `scroll()`, with scroll events as the fallback (F13). `matchMedia('(prefers-reduced-motion)')` (F9). Compositor transform and opacity (F15). SVG `pathLength` drawing (F16). `animateView` on **View Transitions** (F14). `AnimatePresence` relies on React keeping removed children rendered (F18). |
| **GSAP** (+ ScrollTrigger, Flip, Draggable, DrawSVG, MorphSVG) | Timelines and sequencing, scroll storytelling, FLIP, SVG drawing and morphing | **rAF** ticker (F4). Inline style writes of transforms and other properties every frame (F7). `getComputedStyle` reads (F10). **getBoundingClientRect** for Flip and ScrollTrigger (F10). Scroll and resize events (F11, F13). `gsap.matchMedia` with reduced-motion conditions (F9). **Pointer Events** for Draggable (F12). **SVG** path `d`, `getTotalLength`, `stroke-dasharray`/`-dashoffset` (F16). |
| **react-spring** (+ @use-gesture) | Spring physics, interruptible with velocity | **rAF** loop (F4). Inline style and **custom-property** writes (F7). `matchMedia` reduced motion (F9). Scroll events and **ResizeObserver** (F11, F13). **Pointer Events** with pointer capture (F12). |
| **AutoAnimate** | Zero-config enter, exit and move animation of list children | **MutationObserver** on the parent (F11). **getBoundingClientRect** and `getComputedStyle` (F10). **WAAPI** `element.animate` with transform, opacity, width and height keyframes, plus `finished` (F3). It re-inserts removed nodes absolutely positioned to animate them out. `matchMedia` reduced motion (F9). |
| **tailwindcss-animate** / tw-animate-css (shadcn) + Radix `Presence` | Enter/exit of dialogs, popovers, menus, toasts and accordions | **CSS @keyframes** `enter`/`exit` driven by **custom properties** (`--tw-enter-opacity`, `--tw-enter-translate-x`, …) inside keyframes (F2, F7). The `animation` shorthand and fill mode (F2). `data-[state=open]` **attribute selectors** (F8). `motion-safe`/`motion-reduce` → **`@media (prefers-reduced-motion)`** (F9). Radix `Presence` reads `getComputedStyle(node).animationName` and waits for `animationstart`/`animationend`/`animationcancel` before unmounting (F2, F10). The accordion animates height through a measured `--radix-accordion-content-height` (F7, F10). |
| **lottie-web** | After Effects animations (icons, illustrations, onboarding) | **rAF** loop (F4). **SVG** renderer: `path d`, group transforms, masks, `clipPath`, gradients, stroke caps, joins and dashes (F16). **Canvas 2D** renderer: paths, Béziers, fill and stroke, `setTransform`, clip, `globalAlpha` (F17). |
| Platform-level page transitions (React 19 `<ViewTransition>`, Astro, Next.js) | Route and state transitions with shared elements | **View Transitions** `document.startViewTransition`, `view-transition-name`, `::view-transition-*` pseudo-elements, and their CSS animations (F14, F2). |

These dependencies come from the packages' current sources and docs, as
known on 13 November 2026. Phase 2 starts each port by pinning the package
version and confirming its platform calls before writing code.

## Platform features: what Div has, what is missing

This is the main deliverable. Each row is a web platform feature, to be
implemented with Chromium's behaviour and pixels and measured with the
comparison harness or WPT. Where a Rust-native surface is clearly better
(typed values, typed handles, futures instead of promises), the web
semantics stay underneath it: the same timing, events, ordering and
interpolation.

Owners: **engine** (div-style, div-layout, div-paint, div-text,
div-engine), **framework** (divui, div-platform, div-macros), **components**
(div-components). Sizes: **S** days, **M** one to two weeks, **L** more.

| # | Platform feature | Packages | Div at `565db46a` | Missing | Owner | Size | Measured by |
| --- | --- | --- | --- | --- | --- | --- | --- |
| F1 | **CSS Transitions**: `transition-*`, automatic start on a computed-value change, `transitionrun/start/end/cancel`, `transition-behavior: allow-discrete`, `@starting-style` | tailwind, Radix, all CSS-styled components | Engine transition effect with Chromium timing, started only by an explicit `Element::transition` (`div-engine/src/animation.rs:758-800`). Fires `animation*` events (probe `gap_transitions_fire_animation_events_not_transition_events`). | The properties, triggering on style change, transition events, discrete transitions, `@starting-style` | engine | L | WPT css-transitions (§Fidelity) |
| F2 | **CSS Animations**: `@keyframes`, `animation-*`, events with `animationName`/`elapsedTime`, `getComputedStyle().animationName` | tailwindcss-animate, Radix Presence, View Transitions | Engine keyframe effects with Chromium's timing model and all easings. Events dispatched, but with no detail on the `Event` (`crates/divui/src/document.rs:1013-1022`). | `@keyframes` and `animation-*` in style rules (RFC 0001 already orders them in the cascade), event detail, named easing keywords | engine | L | WPT css-animations |
| F3 | **Web Animations API**: `element.animate(keyframes, options)` with several properties and implicit from/to; an `Animation` with play, pause, reverse, finish, cancel, `currentTime`, `startTime`, `playbackRate`, `updatePlaybackRate`, `finished`/`ready`, `onfinish`, `persist`, `commitStyles`; replaceable-animation removal; `getAnimations()`; `document.timeline`; `iterationComposite` | Motion, AutoAnimate (and the base of F1/F2) | Single-property `Element::animate` with typed keyframes (`crates/divui/src/element.rs:364`). Pause, play, seek, rate, finish and cancel by `AnimationId` on `Document` (`document.rs:890-990`). Composite replace/add/accumulate. | An `Animation` handle, multi-property effects, implicit keyframes, `reverse`, `startTime`, `updatePlaybackRate`, `finished` as a future, `commitStyles`, `persist` and auto-removal, `getAnimations`, iteration composite | framework (API) + engine (effect model) | M | WPT web-animations |
| F4 | **`requestAnimationFrame`**: callbacks before style each frame, with the frame's timestamp, cancellable, and throttled when hidden | Motion, GSAP, react-spring, lottie-web | None. The nearest is `PlatformEvent::Presented`, which runs after present, is only on `App::builder()` (`crates/divui/src/app.rs:198`), and needs something else animating to keep frames coming. | All of it, including headless `render_at(t)` running the callbacks at `t` | framework | S–M | WPT `html/webappapis/update-rendering` subset; package comparisons |
| F5 | **Frame production**: vsync-paced frames at the display's refresh rate, frame timestamps, high-resolution timers, throttling of occluded windows | all | Redraws chain while animating (`div-platform/src/desktop.rs:827-829`). Swap interval 1 on GL only, unthrottled softbuffer, wall-clock time at redraw start. `sleep(1 ms)` takes 15.4 ms (§Timers). Minimized windows are excluded, occluded ones are not. | Pacing on every backend at the real refresh rate, present-time stamps, high-resolution timers, occlusion | framework (platform) | M | `examples/frame_pacing.rs` |
| F6 | **Interpolation coverage and raster parity** | all | 48 length, 13 color, 12 number, 12 integer, 4 length-list and 1 transform interpolable property. Transform lists use matrix decomposition. Animated rotate and translate are pixel-exact against Chromium. | `visibility` (Div flips it at p = 0.5 instead of applying the CSS rule; confirmed in pixels). Individual `translate`/`rotate`/`scale`. `filter` lists. `box-shadow`. `clip-path`. `display`/allow-discrete. `interpolate-size`. **Animated fractional `scale()` edge pixels** (the static scale is exact). | engine (style, paint) | S each for visibility and scale parity, M each for the rest | WPT interpolation tables, sampled-time probe |
| F7 | **CSS custom properties**: `var()` including inside `@keyframes`, `@property` registration, `style.setProperty('--x')` | tailwindcss-animate, Radix, react-spring, Motion | Rejected: "custom properties are not supported yet" (`div-style/src/declaration_block.rs:89-91`). Dynamic `style:` needs typed values (sample-git gap 3). | All | engine | L | WPT css-variables, css-properties-values-api |
| F8 | **Selectors and cascade**: class and attribute selectors (`[data-state=open]`), state-driven restyle | tailwindcss-animate, Radix | None: inline styles only (`docs/status.md:19`). Alpha work A1 (typed style rules) is in progress. | As planned in A1 | engine | L (A1) | census |
| F9 | **Media queries**: `@media (prefers-reduced-motion)`, `matchMedia` with change events, and the OS setting | all | A manual `Document::set_prefers_reduced_motion`. With it, finite animations jump to their end (`div-engine/src/accessibility.rs:215-223`). No OS query, no media queries. | Media queries, `matchMedia`, reading `SPI_GETCLIENTAREAANIMATION` and the Linux portal setting plus their change notifications | engine (MQ) + framework (platform) | M + S | WPT mediaqueries |
| F10 | **Geometry and computed style**: `getBoundingClientRect` after transforms, `getComputedStyle`, offset metrics | Motion, GSAP Flip, AutoAnimate, Radix | **Works.** `bounding_rect()` flushes layout and returns the transformed rect (probe `transform_animation_moves_the_layout_box_read_by_bounding_rect`). Typed `computed_style()` returns animated values. | Typed `animation_name` in computed style (with F2) | — | — | census |
| F11 | **Observers**: ResizeObserver, IntersectionObserver, MutationObserver; resize and scroll events | Motion, AutoAnimate, react-spring, GSAP | Scroll events only (`crates/divui/src/scroll_notification_tests.rs`). No observer of any kind in divui or the engine. | ResizeObserver and IntersectionObserver with HTML's update-the-rendering ordering; MutationObserver | framework + engine | M each | WPT resize-observer, intersection-observer, dom/nodes |
| F12 | **Pointer Events**: `pointer*` events, pointer capture, `touch-action`, coalesced events | Motion, react-spring/@use-gesture, GSAP Draggable | `set_pointer_capture` and `dispatch_pointer_event` exist (`element.rs:1122`, `document.rs:1056`). Headless dispatch produces no `pointer*` events (sample-git gap 7). | Owned by the **interactions** sample; motion needs velocity from event timestamps | framework | (interactions) | WPT pointerevents |
| F13 | **Scroll-driven animations**: `animation-timeline: scroll()/view()`, `animation-range`, `ScrollTimeline`/`ViewTimeline` | Motion `scroll()`, GSAP ScrollTrigger (scroll-event path) | **Engine has it:** scroll and view timelines with named ranges (`element.rs:381-558`, `div-engine/src/animation_timeline.rs`). Not Chromium-qualified (`docs/v02/animations.md:148-149`). | CSS syntax (with F2) and timeline objects in the F3 API | engine + framework | M | WPT scroll-animations |
| F14 | **View Transitions**: `startViewTransition`, `view-transition-name`, snapshots, `::view-transition-*` pseudo-elements | Motion `animateView`, React `<ViewTransition>` | None | All. Needs element snapshots as layers (F15) and pseudo-element trees. | engine + compositor | L | WPT css-view-transitions |
| F15 | **Compositor-driven animation**: transform, opacity and filter frames without a style re-cascade, paint re-record or full re-raster; damage; layer promotion (`will-change`, running animations); off-main-thread curves; cheap animation start | all | Transform and opacity skip layout (`Composite` invalidation, `div-engine/src/lib.rs:2880-2886`). Every frame still re-cascades, re-records the whole paint, reports full-viewport damage and rasters the full viewport. Starting an animation re-cascades every root: 16 ms per `animate()` with 1000 running. | Targeted style sampling, damage and partial raster/present, layers, off-thread curves (a Div release blocker, `docs/architecture/threading-model.md:66-67`), O(1) start | engine + compositor + framework | S–M (start cost), M (targeted style, damage), L (layers, off-thread) | `measure_frame_cost`, `frame_pacing` |
| F16 | **SVG**: stroked paths, `stroke-dasharray`/`-dashoffset`, `pathLength`, `getTotalLength`/`getPointAtLength`, gradients, masks, `clipPath`, SVG attribute animation | lottie-web, GSAP DrawSVG/MorphSVG, Motion path drawing | Only filled rects and axis-aligned paths (known gap) | All | engine (paint) | L | WPT svg; shared with **charts** |
| F17 | **Canvas 2D** | lottie-web canvas renderer | None (the `canvas` hits are the canvas background) | All | engine + framework | L | WPT html/canvas; owned by the **canvas** sample |
| F18 | **Retaining removed children**: a component can keep a removed keyed child mounted until its exit animation finishes. This is React's model, and `AnimatePresence` and Radix `Presence` depend on it. | Motion, Radix, AutoAnimate (re-insertion) | `Show`/`For` unmount at once (`crates/divui/src/renderer.rs`). Whether a component can retain children through `DynChild` is unverified. | Verify first; if not possible, a renderer hook. Not a web platform feature but the React-equivalent contract. | framework | S–M | probe (Phase 2) |

**Order of value:**
- **F4 and F3** unblock the imperative packages: Motion, react-spring,
  AutoAnimate and GSAP core.
- **F7, F8, F2 and F9** unblock the shadcn enter/exit stack. These are
  already the alpha plan's A1 path.
- **F15** decides whether any of it is smooth.
- **F14, F16 and F17** are large and can follow.

**Platform decisions accepted on 13 November 2026.** The user accepted
the platform plan's design decisions (`decisions-2026-11-13.md`, resolving
plan questions Q1–Q16). The ones that change this design:

- **One rendering design (Q2).** F15's damage and layers are not a
  motion-only design. The engine designs partial redraw once, in an RFC:
  damage regions first, then layers. It is Tier 0 item 0.7, together with
  R5 (targeted sampling, O(1) start). Animation layers (including the
  raster scale behind the fractional-`scale()` difference), off-thread
  curves and the View Transitions snapshots of F14 are built on top of it,
  as are the canvas samples' canvas slots. Motion contributes requirements
  and measurements, not a second design.
- **One Web Animations surface (Q12).** F3 is the single animation API.
  Animations created by CSS (F1, F2) appear in `getAnimations()`. The
  current single-property `Element::animate → AnimationId` is retired once
  its users migrate. This repository's A3 scaffolds are among those users
  and migrate when the surface lands.
- **Runtime CSS (Q3).** Compile-time strict checking stays for code
  written for Div. Third-party sheets, such as tw-animate-css, go through
  a browser-compatible ingestion mode that drops what Chromium drops and
  reports it. Runtime custom-property writes (`setProperty('--x')`, F7)
  are supported, which react-spring and Radix's accordion height need.
- **SVG earlier (Q9).** Inline SVG (F16, plan V1) starts right after the
  engine's Tier 0 items, which brings lottie-web's SVG renderer and GSAP
  DrawSVG closer.
- **Retained children (Q15).** F18 is accepted as framework work outside
  the web-feature rule, as a non-web contract that `AnimatePresence`
  needs. It is not scheduled yet.
- **Deterministic capture (Q13).** This repository owns the measurement:
  Chromium's byte stability across runs, measured before choosing between
  begin-frame control and virtual time (milestone M2).

## Probes

| Probe | What it shows | Run |
| --- | --- | --- |
| [`tests/probe_animation_api.rs`](../tests/probe_animation_api.rs) | 16 assertions on the public animation API: properties, easing, timing, control, events, interruption, reduced motion, headless sampling. `gap_*` tests pin behaviour that differs from CSS/Chromium. | `cargo test --test probe_animation_api` (CI) |
| [`tests/measure_frame_cost.rs`](../tests/measure_frame_cost.rs) | Per-frame cost of N animating elements, split into sample / update / raster. | `cargo test --release --test measure_frame_cost -- --ignored --nocapture --test-threads 1` |
| [`examples/frame_pacing.rs`](../examples/frame_pacing.rs) | Native frame intervals, imperative per-frame motion, UI-thread stalls, timer accuracy. | `cargo run --release --example frame_pacing -- <mode> <count> <seconds> <backend> <block_ms>` |
| [`tests/fidelity_sampled.rs`](../tests/fidelity_sampled.rs) + [`tools/chromium_sampled.py`](../tools/chromium_sampled.py) | Div vs pinned Chromium 147 pixels at sampled times. | see the files' headers |

## What Div has

### The animation engine

Animation in Div is **typed and imperative**. There is no `transition`,
`animation`, `@keyframes` or `will-change` in the style schema
(`docs/v02/animations.md:4-5`; probe
`gap_no_css_animation_transition_or_motion_properties`). Instead:

- `Element::animate(property, Keyframes<T>, AnimationOptions)` starts a
  keyframe effect on **one property** (`crates/divui/src/element.rs:364`;
  engine `crates/div-engine/src/animation.rs:721`, `802-920`). Keyframes are
  ordered offsets 0..1 with optional per-keyframe easing
  (`crates/div-style/src/animation.rs:791-840`).
- `Element::transition(property, to, options)` is an explicit transition:
  the engine captures the current computed value as the start and stages
  `to` as the authored value (`div-engine/src/animation.rs:758-800`). A plain
  style change never transitions (probe
  `gap_setting_a_style_never_starts_a_transition`).
- `AnimationOptions` has delay (negative allowed), duration, iterations
  (fractional or infinite), direction, fill, play state, playback rate,
  composite (replace/add/accumulate) and easing
  (`div-style/src/animation.rs:382-408`). Phase and progress follow
  Chromium's timing model, including its 1 µs boundary snap
  (`animation.rs:523-633`). There is no iteration composite.
- **Easing**: linear, `cubic-bezier()` (a port of Chromium's
  `gfx::CubicBezier`), `steps()` with all four jump positions, and
  `linear()` with stops (`animation.rs:63-328`). No named `ease`/`ease-in`/…
  constants. Probes `cubic_bezier_ease_matches_the_css_curve`,
  `steps_support_every_jump_position`,
  `linear_function_with_stops_is_supported`.
- **Interpolable properties** come from per-property metadata
  (`InterpolationKind`, `div-style/src/property.rs:82-101`): 48 lengths,
  13 colors, 12 numbers, 12 integers, 4 length lists, 1 transform, and 168
  discrete. Discrete values flip at progress 0.5
  (`animation.rs:1153-1157`). Transform lists interpolate pairwise with
  matrix decomposition for mismatched suffixes
  (`div-style/src/transform_composition.rs:1420-1498`).
- **Controls** by `AnimationId` on `Document`: pause, play, seek (time or
  progress), playback rate, finish, cancel, state
  (`crates/divui/src/document.rs:890-990`). No `reverse()`, no handle
  object, no `finished` future, no `getAnimations()`.
- **Events**: `animationstart`, `animationiteration`, `animationend`,
  `animationcancel` are dispatched through capture/target/bubble, but as an
  empty event: property and elapsed time come only from
  `Document::drain_animation_events` (`document.rs:992-1030`). Transitions
  fire the same `animation*` events; there are no `transition*` events
  (probe `gap_transitions_fire_animation_events_not_transition_events`).
- **Scroll and view timelines** with named `animation-range`s:
  `Element::animate_on_scroll`, `animate_on_view` and their range variants
  (`element.rs:381-558`; `div-engine/src/animation_timeline.rs`).
- **Reduced motion**: a manual `Document::set_prefers_reduced_motion`;
  finite animations then jump to their end and `is_animating` is false
  (`div-engine/src/accessibility.rs:215-223`; probe
  `reduced_motion_is_a_manual_document_flag_that_jumps_to_the_end`).
- **The clock is manual and absolute**: `Document::advance_time(t)` samples
  every effect at document time `t` (`document.rs:866-880`). Headless code
  can render any time deterministically, backwards too (probe
  `headless_render_at_samples_any_time_deterministically`). Note
  `TestApp::render_png()` always renders at t = 0
  (`crates/divui/src/testing.rs:162-165`); use
  `HeadlessApp::render_at(t)` or `advance_time` + `render_to_bitmap`.

The engine has 11 animation test suites with 138 tests
(`crates/div-engine/tests/animation_*.rs`). Div's own doc says much of the
timing and timeline work is not yet Chromium-qualified
(`docs/v02/animations.md:31-32`, `148-149`).

### The frame loop

- One winit loop per app, `ControlFlow::Wait`
  (`crates/div-platform/src/desktop.rs:1463`). A window wants a frame when
  `is_animating() || needs_redraw()` (`desktop.rs:827-829`); after each
  redraw the loop immediately requests the next one
  (`desktop.rs:1394-1399`, `1427`), in fair rounds across windows
  (`1113-1124`, `1253-1271`).
- **Pacing**: the only throttle is `swap_buffers` with a requested swap
  interval of 1 on the GL path (`desktop.rs:232-235`, error ignored). The
  software (softbuffer) path has none. The GL path is not GPU rendering: it
  uploads the CPU-rastered frame with a full-size `TexSubImage2D` and draws
  one quad (`desktop.rs:251-321`).
- **Frame time**: `started.elapsed()` at the start of the redraw
  (`desktop.rs:860`), passed to `Document::begin_frame`. Not a vsync or
  present timestamp.
- **No frame callback**: nothing like `requestAnimationFrame`. The nearest
  hook is `App::on_platform_event` receiving `PlatformEvent::Presented
  { frame_number, time_ms }` after present (`crates/divui/src/app.rs:198`,
  `desktop.rs:906-910`), only on the `App::builder()` path (`AppSpec`
  windows build their `App` internally, `app_spec.rs:142`), and only while
  something else keeps frames coming.
- **Timers**: `task::time::sleep`/`interval` wrap `async_io::Timer`
  (`crates/divui/src/task.rs:155-179`); no `timeBeginPeriod` anywhere.
- **Background windows**: minimized windows leave the frame rounds
  (`desktop.rs:831-835`); `WindowEvent::Occluded` is ignored
  (`desktop.rs:1096`). The clock is wall time, so animations continue (as in
  CSS) and are not throttled when occluded.

### The compositor

- Transform and opacity have the `Composite` invalidation class
  (`div-style/src/generated_properties.rs:2578-2583`, `2808-2813`), which
  sets only `dirty.compositing` (`div-engine/src/lib.rs:2880-2886`). Layout
  is skipped: retained transforms and overflow are refreshed in place
  (`div-engine/src/lib.rs:2564-2585`).
- But every animated frame still: re-cascades all authored styles
  (`div-engine/src/animation.rs:1659-1661`), re-records the **whole** paint
  (`lib.rs:2675-2681`), reports **full-viewport** damage
  (`lib.rs:2686-2691`), rasters the full viewport on the CPU
  (`crates/div-paint/src/render.rs:645-700`) and presents the full frame.
- `div-compositor` runs only `SoftwareCompositor::render`
  (`crates/div-compositor/src/lib.rs:333-354`). Layer records exist as
  metadata but `authored_layers_are_partitioned()` is always false
  (`retained_scene.rs:318-321`); the render-thread `SceneMailbox` is used
  only in tests. `will_change_transform` only affects how paint groups are
  rastered to match Chromium (`div-paint/src/painter.rs:1722-1730`).
- Ganesh GPU raster is behind a feature that divui does not enable
  (`div-compositor/Cargo.toml:13-15`).
- Div's own docs: compositor-thread animation is a release blocker and the
  "100 promoted layers at 60 Hz with a blocked UI thread" gate is not met
  (`docs/architecture/threading-model.md:42-44`, `66-67`;
  `docs/v02/animations.md:166-175`).

## Measurements

Measured, not claimed: each number below holds for the command and the
conditions next to it. **Conditions for all runs:** 13 November 2026,
Windows 11 Pro 26300, 24 logical cores, RTX 3090, primary display
5120 × 1440 at 239 Hz and 96 DPI (scale 1.0), release profile, Div
`565db46a`. **Other sessions were compiling at the same time**: CPU load
read 83–100 % before and after the runs. Treat the numbers as an upper bound
under load and re-run on an idle machine before comparing builds.

### Per-frame cost, headless

`cargo test --release --test measure_frame_cost -- --ignored --nocapture
--test-threads 1`. Viewport 1280 × 800 at 1×, N cells of 24 × 24 px in a
wrapping flex row, each with an infinite alternating animation of one
property, phases spread by negative delays. 120 frames at 60 Hz steps after
10 warm-up frames; milliseconds, median / p95.

| property | N | sample (style) | update (layout + paint record) | raster (full viewport) | total |
| --- | ---: | ---: | ---: | ---: | ---: |
| transform | 1 | 0.11 / 0.19 | 0.15 / 0.22 | 6.65 / 7.94 | 6.95 / 8.21 |
| transform | 10 | 0.29 / 0.42 | 0.24 / 0.34 | 6.70 / 8.24 | 7.40 / 8.95 |
| transform | 100 | 2.56 / 3.24 | 1.34 / 1.76 | 7.32 / 9.42 | 11.42 / 13.46 |
| transform | 1000 | 45.56 / 66.92 | 21.75 / 31.63 | 11.82 / 20.66 | 80.42 / 108.78 |
| opacity | 1 | 0.10 / 0.16 | 0.15 / 0.27 | 7.57 / 9.89 | 7.84 / 10.13 |
| opacity | 10 | 0.27 / 0.63 | 0.26 / 0.82 | 8.40 / 15.34 | 9.03 / 15.92 |
| opacity | 100 | 1.97 / 8.28 | 0.93 / 1.66 | 7.72 / 15.04 | 10.93 / 21.72 |
| opacity | 1000 | 29.35 / 33.52 | 10.51 / 12.79 | 7.75 / 9.62 | 48.08 / 55.00 |
| background-color | 1 | 0.10 / 0.15 | 0.13 / 0.20 | 6.37 / 7.26 | 6.60 / 7.54 |
| background-color | 10 | 0.25 / 0.38 | 0.20 / 0.28 | 6.53 / 7.53 | 6.99 / 8.08 |
| background-color | 100 | 1.87 / 2.33 | 0.74 / 0.96 | 6.67 / 8.08 | 9.33 / 10.93 |
| background-color | 1000 | 31.31 / 42.50 | 9.53 / 13.19 | 8.10 / 11.43 | 49.84 / 62.52 |
| width (layout) | 1 | 0.10 / 0.20 | 0.17 / 0.31 | 7.06 / 11.06 | 7.34 / 11.44 |
| width (layout) | 10 | 0.25 / 0.46 | 0.30 / 0.43 | 7.06 / 9.16 | 7.63 / 10.16 |
| width (layout) | 100 | 1.94 / 2.45 | 1.05 / 1.43 | 7.26 / 8.87 | 10.35 / 12.49 |
| width (layout) | 1000 | 30.90 / 38.26 | 12.66 / 16.49 | 8.35 / 10.16 | 52.78 / 64.37 |

A static page re-rastered with nothing animating: median 7.15 ms, p95
9.44 ms (`idle_frame_cost_with_no_animation`).

What the table shows, without claims beyond it:

- **Raster sets a floor** of about 6.4–8.4 ms median per frame at
  1280 × 800 whatever N is, because every frame rasters the full viewport
  (F15). One animating 24-px square costs as much raster as the whole page.
- **Style sampling grows faster than N**: 100 → 1000 elements is ×17.8
  (transform) and ×16.7 (background-color) in the sample stage. Each frame
  re-cascades every root (F15).
- **Transform frames were not cheaper than width frames** in the update
  stage at N = 1000 (21.75 vs 12.66 ms median) in this layout, although
  transform skips layout: the retained-transform refresh and the full paint
  re-record dominate. At N ≤ 100 they are within 0.3 ms.
- The per-stage split stops at Div's public API: layout and paint recording
  are not separable from outside: Div exposes no lifecycle statistics.

### Starting animations

`start_cost_by_animation_count` in the same file: build N cells, then the
same N cells with one `animate()` each; the difference divided by N.

| N | cells only (ms) | cells + `animate` (ms) | per `animate` (µs) |
| ---: | ---: | ---: | ---: |
| 100 | 145.9 | 244.5 | 986 |
| 300 | 145.9 | 1 387.8 | 4 140 |
| 1000 | 205.8 | 16 183.9 | 15 978 |

The cost of one `animate()` grows linearly with the number of running
animations, so starting N animations is quadratic: 1000 staggered list items
take 16 s to start. Natively, `frame_pacing engine 1000` presented its first
frame 18 056 ms after process start. **F15** (engine, S–M): starting an
animation should not re-cascade every root, or starts should batch to one
re-cascade per frame. This also bounds staggered lists and `Presence` with
many children.

### Native frame pacing

`cargo run --release --example frame_pacing -- <mode> <count> <seconds>
<backend> <block_ms>`, window 1280 × 800 at 1×, first 0.5 s skipped.
Intervals between `PlatformEvent::Presented` events (the frame clock; the
handler's `Instant` agrees within 0.3 ms everywhere).

| run | frames | mean interval | median | p95 | p99 | max | > 25 ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| engine, 100 cells, auto (GL) | 281 | 14.97 (66.8/s) | 14.85 | 16.69 | 17.92 | 18.35 | 0 |
| engine, 100 cells, gl | 301 | 14.81 (67.5/s) | 14.71 | 16.51 | 17.27 | 18.02 | 0 |
| engine, 100 cells, software | 300 | 15.73 (63.6/s) | 15.60 | 17.71 | 20.13 | 21.27 | 0 |
| engine, 300 cells, auto | 265 | 23.21 (43.1/s) | 23.10 | 25.88 | 26.51 | 26.89 | 31 |
| engine, 1000 cells, auto, 15 s | 1 frame in 15 s (first frame at 18.1 s) | — | — | — | — | — | — |
| signal (per-frame `set_property` on 100 cells from `Presented`, keep-alive animation) | 308 | 14.94 (66.9/s) | 14.84 | 17.03 | 18.66 | 20.02 | 0 |
| engine, 100 cells, 50 ms UI-thread block every 30 frames | 273 | 16.23 (61.6/s) | 14.49 | 17.51 | 64.81 | 65.54 | 9 |

- On a 239 Hz display both backends ran at about 64–68 frames/s with 100
  cells: frames were **CPU-bound**, not paced by vsync (a 239 Hz vsync is
  4.2 ms). Under this load the softbuffer path was not faster than GL;
  the ~690 frames/s the framework track recorded for one idle window
  (`docs/progress/tracks/framework.md:396-400`) was not reproduced here
  because raster dominated.
- Every UI-thread block shows up as a frame of the block's length: there
  is no off-thread animation (F15).
- Imperative per-frame motion from the `Presented` callback kept the same
  rate as engine animations at 100 cells, but needs an infinite
  "keep-alive" engine animation to keep frames coming and lands one frame
  late (it runs after present). That is a scaffold; the platform feature is `requestAnimationFrame` (F4).

### Timers

`frame_pacing timers 0 6`, no animation (1 frame presented while idle):

| timer | n | mean | median | p95 | p99 | max | min |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `task::time::interval(16 ms)` tick spacing | 186 | 16.02 | 15.66 | 27.82 | 32.12 | 32.41 | 0.00 |
| `task::time::sleep(1 ms)` actual duration | 217 | 13.87 | 15.40 | 16.33 | 16.68 | 17.96 | 1.21 |

`sleep(1 ms)` took 15.4 ms at the median, which is the default Windows
timer tick of about 15.6 ms: timers cannot pace animation on Windows (F5:
high-resolution timers; and motion should run from `requestAnimationFrame`,
F4, not from timers).

## Fidelity

### Sampled-time probe against pinned Chromium

`cargo test --test fidelity_sampled -- --ignored` writes Div's frames with
`advance_time(t)`; `python3 tools/chromium_sampled.py <div checkout>
<chrome dir>` (in WSL, Chromium 147.0.7727.50) captures the same box with a
CSS animation sampled by `animation-delay: -t; animation-play-state:
paused`, through Div's own `render_chrome` (the census capture). 200 × 120
at 1×, a 60 × 60 blue box, 1000 ms animations, t = 0, 100, 250, 500, 750,
1000 ms. Exact RGBA comparison.

**46 of 54 samples are pixel-identical.**

| case | exact | not exact |
| --- | --- | --- |
| opacity, linear | 6 / 6 | — |
| `translate()` + `rotate()`, `cubic-bezier(0.25, 0.1, 0.25, 1)` | 6 / 6 | — |
| background-color, linear | 6 / 6 | — |
| width, `steps(4, jump-end)` | 6 / 6 | — |
| `scale(1)` → `scale(2)`, linear | 4 / 6 | t = 250: 300 px off by 1; t = 750: 205 px off by up to 191 |
| same, `linear(0, 0.25 75%, 1)` | 4 / 6 | t = 250: 260 px off by 1; t = 750: 300 px off by 1 |
| same, percentage stops | 4 / 6 | identical to the row above |
| static `scale(1.75)`, not animated (control) | 6 / 6 | — |
| visibility hidden → visible, linear | 4 / 6 | t = 100, 250: Div hidden, Chromium visible (F6) |

- **The `visibility` gap (F6) is confirmed in pixels**: Chromium shows the box for 0 < p < 1, Div
  hides it until p = 0.5 (F6).
- **Animated fractional scale (F6, engine: paint), new finding**: an *animated* fractional scale rasters
  differently from Chromium at the box edges, while the same static scale
  is exact. The differences are on edge pixels only. Likely cause, not
  verified: Chromium composites an element with a running transform
  animation as its own layer and picks the layer's raster scale from the
  animation, so edge coverage differs from a direct raster. Div already
  imitates Chromium's layered raster for `will-change: transform` paint
  groups (`div-paint/src/painter.rs:1722-1730`); running animations may need
  the same rule. Rotation and translation matched exactly.
- The 1-level differences under `linear()` easing also appear with plain
  linear easing at the same scale, so they belong to that finding, not to the easing.

### WPT: what the existing porter gives

Run in a private copy of Div with `CHROMIUM_WPT_ROOT` set to the Chromium
checkout's WPT (`E:\open-ui\out\chromium-wpt`, commit `09d377d9`):
`python tools/wpt/port_coverage_area.py <area> <module>`. Nothing was
committed to Div.

| area | files | portable | of those | rejected |
| --- | ---: | ---: | --- | --- |
| css-animations | 238 | 86 | 21 references, 53 `-manual`, 12 others | 145 `uses_javascript`, 3 `::part`, 2 no layout content, 1 `:visited`, 1 `<template>` |
| css-transitions | 186 | 5 | 4 references, 1 crash test | 180 `uses_javascript`, 1 unsupported `transition` |
| css-easing | 13 | 1 | — | 12 `uses_javascript` |

None of the portable files exercises Div's animation engine:
- The porter freezes every animation at t = 0
  (`tools/wpt/port_wpt.py:7657-7664`), and the Chromium template pauses
  them (`tools/accountability/run_all_pixel_comparisons.py:69-71`).
- The porter's only sampling path interpolates `background-color`
  (`port_wpt.py:3186-3226`).

### WPT: the suites for the motion platform

Counted in upstream WPT at `cb1a0cb1` (24 March 2026, the same day as the
Chromium checkout's commit). The counts exclude support files, references,
crash tests and `-expected` files. The Chromium column counts the
`-expected.txt` baselines in the Chromium checkout: files where Chromium
records its own subtest results that differ from a full pass.

| suite | test files | tentative | testharness | reftests (with script) | Chromium baselines | feature |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| css/css-transitions | 180 | 17 | 119 | 6 (5) | 6 | F1 |
| css/css-animations | 207 | 28 | 112 | 29 (15) | 9 | F2 |
| web-animations | 151 | 6 | 133 | 18 (18) | 23 | F3 |
| scroll-animations | 226 | 75 | 193 | 33 (31) | 16 | F13 |
| css/css-view-transitions | 445 | 22 | 135 | 306 (303) | 0 | F14 |

Almost every motion test is a testharness file or a reftest that runs a
script, typically waiting for animation frames before the screenshot. Div
runs no JavaScript, so "port through the harness" means translating the
scripts, not rendering the pages.

### WPT: what would carry real signal

WPT's **interpolation tests** are data, not scripts: 327 files under `css/`
(5 tentative) call `interpolation-testcommon.js` with tables of
`{property, from, to}` and `[fraction → expected]` rows — 1 137
`test_interpolation`, 448 `test_no_interpolation` and 531
`test_composition` blocks. The largest groups are css-transforms (32
files), css-backgrounds (30), motion (21), css-masking (21), css-values (18),
css-sizing (16), css-fonts (16), filter-effects (12).

### Plan

**Platform level: Div against Chromium, per feature.**

1. **Translate testharness scripts to native calls.** Div already has a
   JavaScript-to-native inventory for WPT
   (`tools/wpt/javascript_mutation_ir.mjs`). It parses with acorn,
   recognizes the testharness assertions, and today rejects
   `requestAnimationFrame` as `animation-frame-dependency`. Extend it so that:
   - the Web Animations API calls (`animate`, `getAnimations`,
     `currentTime`, `playbackRate`, `finished`, `effect.getComputedTiming`)
     map onto the F3 API;
   - `getComputedStyle` reads map onto typed computed style;
   - `waitForAnimationFrames(n)` / rAF map onto deterministic frames of the
     F4 clock.

   Each translated subtest becomes a Rust test whose oracle is the
   assertion, adjusted by Chromium's `-expected.txt` baseline where
   Chromium itself fails a subtest. That way Div matches Chromium, not only
   the spec. Order: web-animations (F3), css-animations (F2),
   css-transitions (F1), scroll-animations (F13), then
   css-view-transitions (F14). Count the subtests that translate before
   sizing each suite. (engine tools, M per suite)
2. **Interpolation tables → value and pixel tests.** WPT's 327
   `interpolation-testcommon.js` files are data tables (above), so they
   need no script execution. Generate one Rust test per row: animate the
   property and compare the computed value at each fraction. For visual
   properties, also generate a sampled pixel pair against Chromium.
   (engine tools, M)
3. **Sample at time t in the census harness.**
   - Div side: the porter maps `animation`/`@keyframes` (and, after F1,
     transitions) onto the engine and calls `advance_time(t)` instead of
     freezing at 0.
   - Chromium side: rewrite to a negative delay, as this repository's
     probe does, or use CDP `Animation.seekAnimations`.
   - This makes the 35 css-animations and css-transitions reftests
     meaningful. (engine tools, M)
4. **This repository's sampled-time probe** grows with each feature the
   ports need, so every scaffold has a Chromium check until the WPT ports
   above replace it.

**Package level: the port against the original package.** For each
package, the same scenario runs both ways and the frames are diffed with
the harness's exact RGBA comparison:

- **Chromium side:**
  - The pinned npm package, vendored with a lockfile under `fidelity/web/`,
    runs a scenario page in pinned Chromium 147.
  - Time is made deterministic by driving frames explicitly. Headless
    Chromium's `--enable-begin-frame-control` with
    `HeadlessExperimental.beginFrame` at fixed frame times is one option;
    CDP virtual time (`Emulation.setVirtualTimePolicy`) is the other.
    Which of the two gives byte-stable captures for compositor-driven
    WAAPI animations is the first measurement to make. This is plan
    question Q13 and milestone M2. The decision is to measure before
    choosing.
  - **Measured in M2** ([q13-chromium-capture.md](q13-chromium-capture.md)),
    10 fresh browser runs per row:
    - Virtual time is not byte-stable: 2–8 distinct captures per t.
    - Begin-frame control is not available in the pinned binary.
    - Paused seeking of WAAPI and CSS animations is byte-stable: 1 hash
      per t, equal to the reference.
    - A rAF pump driven by the oracle is byte-stable for rAF-driven code
      in the same way.
    - Package comparisons therefore seek the animations and pump rAF at
      each frame time.
  - Each frame k is captured.
- **Div side:**
  - The Rust port of the package runs the same scenario (a shared JSON
    script: mount, set target, add or remove item, pointer path, scroll
    offset, at frame k).
  - It runs on the headless clock, with `requestAnimationFrame` callbacks
    at the same frame times (F4).
- **Requirements for the port:** f64 arithmetic and the package's own
  formulas, so a pixel difference points at the platform rather than at
  the port. The springs Motion exports as `linear()` easings, for example,
  are compared as the exact easing strings Motion produces.
- **Applies to:** each package in the table above, once its platform
  features exist. Until then, the same scenario compares only the parts
  that run.

## Phase 2: porting the packages

Started 13 November 2026, after the Tier 0 platform work was scheduled in
Div (the consolidated platform plan, §2). Phase 2 pins Div `92ea25a0`.
Ports live in this repository and keep each package's semantics and
formulas. They are built only on Div's public platform surface.

What Div scheduled that motion depends on:

- **R1** `requestAnimationFrame` with **T1** frame advance (replaces F4).
- **R4** damage and partial raster, with **R5** targeted sampling and
  O(1) animation start (F15 steps 1–2).
- **S1** selectors, and **S3/S4** custom properties and media (F7, F8, F9).
- **O1** `ResizeObserver` and **I1** Pointer Events (part of F11, F12).
- **T3** WPT script translation, which is what lets the testharness-based
  animation suites be measured.

`@keyframes` and transitions from rules (**A2**, **A1**), the Web
Animations API surface (**A3**), MutationObserver and IntersectionObserver
(**O3**, **O2**), and frame pacing (**R2**) are Tier 1 and not scheduled.
SVG (**V1**) and Canvas 2D (**C2**) are later still. Ports therefore start
with what runs on Div's existing animation engine, and the rest sits behind
labelled scaffolds.

Milestones. Each is one PR, merged before the next starts.

| M | PR | Content | Platform features used | Scaffolds (`div-gap`) | Measured by |
| --- | --- | --- | --- | --- | --- |
| M1 | repin and plan | Pin Div `92ea25a0`, rerun the probes, write these milestones and the gap-ID mapping | — | — | probes, CI |
| M2 | deterministic Chromium capture (plan Q13) | Measure the byte stability of three ways of sampling an animation in pinned Chromium: negative delay with a paused animation, virtual time, and `--enable-begin-frame-control` with `HeadlessExperimental.beginFrame`. Cover a CSS animation and a rAF-driven JS animation. | — | — | repeated captures, distinct-hash counts |
| M3 | Motion `animate()` WAAPI core | Transition resolution, the easing set, the spring generator with its `linear()` export, for `opacity` and `transform`. Motion 14.14.0 vendored. Done: [motion-port.md](motion-port.md) | Engine keyframe effects, `cubic-bezier()`, `linear()` (`Easing::LinearStopsPercent`), per-keyframe offsets and easings | A3 Web Animations API (one `Element::animate` per property stands in for the `Animation` handle and `finished`; migrates when the single surface lands, per Q12) | the `element.animate()` calls against Motion's own (52 cases, exact); sampled frames against Motion in Chromium (90 of 90 exact) |
| M3b | Motion `stagger` and sequences | `stagger()` and `animate(sequence)` for `opacity` and `transform`. Done: [motion-port.md](motion-port.md#m3b-stagger-and-sequences). `filter` and `clipPath` stay unported: Div has no CSS `filter` lists or `clip-path` shapes (A4) | Engine keyframe effects with offsets and per-keyframe easings, computed opacity | A3 (the M3 scaffold; no new one) | the `element.animate()` calls against Motion's own (37 cases, 111 calls, exact); sampled frames against Motion in Chromium (128 of 128 exact) |
| M4 | react-spring core | The spring integrator, interruption with velocity, and chained springs | Inline style writes, `bounding_rect` | R1 requestAnimationFrame (`PlatformEvent::Presented` plus a keep-alive animation on the desktop; the test drives frames on the headless clock) | per-frame values and sampled frames against react-spring in Chromium |
| M5 | GSAP core tween and timeline | Tweens, the timeline, eases and stagger. Before porting, the GSAP licence terms are checked and recorded. | Inline style writes | R1 requestAnimationFrame | per-frame values and sampled frames against GSAP in Chromium |
| M6 | AutoAnimate | FLIP enter, exit and move for list children | `bounding_rect` after transforms, engine keyframe effects | O3 MutationObserver; A7 retained removed children; A3 `finished` | sampled frames of add, remove and reorder against AutoAnimate in Chromium |
| M7 | tailwindcss-animate keyframes | The `enter`/`exit` keyframes with their default values, and reduced motion | Engine keyframe effects; `set_prefers_reduced_motion` | A2 `@keyframes`; S3 custom properties in keyframes; S1 `data-[state]` selectors; S4 `prefers-reduced-motion` media | sampled frames against the tw-animate-css stylesheet in Chromium |
| M8 | Tier 0 remeasure | After R1, R4 and R5 land: delete the R1 scaffolds, rerun `measure_frame_cost`, `start_cost_by_animation_count` and `frame_pacing`, and update the gap list | R1, R4, R5 | — | the same commands as in Phase 1 |

Blocked and not scheduled: **lottie-web** (V1 SVG, which starts right
after the engine's Tier 0 per Q9, and C2 Canvas 2D), Motion's
`animateView` (A6 View Transitions), Motion's `scroll()` (A5 scroll-driven
syntax) and Radix `Presence` (A2 events with `animationName`, and A7,
which Q15 accepted as framework work).

Each scaffold carries `// div-gap: <plan ID> <web feature>`. It is listed in
[framework-gaps.md](framework-gaps.md) with the feature that replaces it,
and it is deleted when Div ships that feature. Each port gets the
package-level fidelity comparison above. Results are reported as measured,
with the command and the conditions.
