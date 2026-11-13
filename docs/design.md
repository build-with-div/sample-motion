# Sample Motion: design

A dogfood sample for **animation and motion** on
[Div](https://github.com/build-with-div/div), using only Div's public API.
Phase 1 (this document) answers what it takes for Div to support the motion
of a professional desktop app: what works today, what has a workaround, what
is missing, who owns each gap and how big it is. Phase 2 builds a motion
gallery on Div's public API with labelled `// div-gap:` workarounds; it
starts only on the go-ahead.

Everything below was checked against Div **`565db46a`** (build-with-div/div
`main`, 13 November 2026) by reading the source and by running the probes in
this repository on Windows 11 (24 cores, RTX 3090, 5120-px display at
239 Hz). File references are to the Div tree at that commit. The proposed
APIs are in [rfc-motion.md](rfc-motion.md).

## Summary

- Div's animation **engine** is further along than its API: typed keyframe
  effects with Chromium's timing model, every CSS easing function,
  composition, controls, scroll and view timelines and a deterministic
  clock. In a sampled-time comparison with pinned Chromium, 46 of 54 frames
  were pixel-identical; the 8 others are two engine gaps (M8, M30).
- What an app author needs around it is **missing**: CSS syntax and
  automatic transitions, a frame callback, a handle type, presence,
  springs, FLIP helpers, and reading the OS reduced-motion setting.
- **Frames are expensive**: every animated frame re-cascades all styles,
  re-records the whole paint and rasters the full viewport on the CPU (about
  7 ms at 1280 × 800 for one animated square, measured under load). Nothing
  is vsync-paced at the display rate, timers tick at 15.6 ms on Windows,
  and starting N animations is quadratic.
- Most app-level motion can be built **in divui on the existing engine**
  (RFC §2): springs baked into `linear()` easings keep motion engine-driven,
  deterministic and comparable with Chromium.

### Div work list

| Owner | Item | Gaps | Size |
| --- | --- | --- | --- |
| framework | Frame callback before style, with frame timestamps; `AppSpec` access | M12, M21 | S–M |
| framework (platform) | Vsync pacing at the display rate on every backend; occluded-window throttling; high-resolution timers | M11, M13, M14 | M |
| framework (platform) | OS reduced-motion setting and change notification | M27 | S |
| framework | `Animation` handle, `finished`, `reverse`, `getAnimations`; event details on `Event`; public frame/lifecycle stats | M5, M6, M15 | S |
| framework (macros) | `transition:` directive in `view!`; element refs | M2, M25 | S–M |
| framework | `divui::motion`: springs and decay baked into `linear()`, `Presence`, stagger, FLIP/shared-element helpers | M20, M22, M23, M24 | M each |
| components | Use `Presence` in Dialog, Popover, Tabs indicator, Toast | M23 | S each |
| engine | Named easings; multi-property effects | M3, M4 | S |
| engine | `visibility` interpolation; `transition*` events | M8, M5 | S |
| engine | Animation start without a full re-cascade | M29 | S–M |
| engine (paint) | Animated fractional-scale raster parity | M30 | S–M |
| engine | Individual transform properties; `filter`/`box-shadow`/`clip-path` interpolation | M7, M9 | M each |
| engine | Targeted style sampling; damage and partial raster/present | M16, M17 | M each |
| engine + compositor | Layer promotion and cached layer raster; off-thread curves | M18, M19 | L |
| engine | CSS `transition-*`/`animation-*`/`@keyframes` (RFC 0001); custom properties; media queries | M1, M10, M28 | L |
| engine tools | WPT interpolation tables → value tests; sample-at-time in the census harness | — | M each |

Areas: **engine** (div-style, div-layout, div-paint, div-text, div-engine),
**framework** (divui, div-platform, div-macros, div-cli), **components**
(div-components). Sizes: **S** days, **M** one to two weeks, **L** more.

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

## Works / workaround / missing

`M` numbers are this sample's gap numbers (`div-gap: M<n>` in code).

### CSS transitions and animations

| # | Capability | State | Evidence / workaround | Owner | Size |
| --- | --- | --- | --- | --- | --- |
| — | Keyframe effects on typed properties (opacity, transform, colors, lengths) | **works** | `Element::animate`; probes | — | — |
| — | Timing: negative delay, fractional/infinite iterations, direction, fill, play state, rate, composite | **works** | probe `delay_fill_iterations_and_direction_follow_css_timing`, `additive_composition_stacks_transforms` | — | — |
| — | Easing: `cubic-bezier`, `steps` (4 jumps), `linear()` stops | **works** | probes | — | — |
| — | Pause, play, seek, rate, finish, cancel | **works** by id | probe `pause_seek_rate_and_cancel_are_available_by_id` | — | — |
| — | Scroll- and view-linked animation with ranges | **works** (native API; not Chromium-qualified) | `animate_on_scroll`, `animate_on_view` | — | — |
| M1 | `transition-*`, `animation-*`, `@keyframes` (CSS syntax) | **missing** | Typed calls only. Workaround: call `animate`/`transition` from code. RFC 0001 already orders them in the cascade. | engine | L |
| M2 | Automatic transitions on style change | **missing** | Workaround: call `Element::transition` instead of `set_property`; a `transition:` directive in `view!` could do it (RFC §2.2) | framework (macros), then engine | S–M |
| M3 | Named easings `ease`, `ease-in`, `ease-out`, `ease-in-out` | **missing** | Workaround: `Easing::cubic_bezier(0.25, 0.1, 0.25, 1.0)` etc. | engine | S |
| M4 | Several properties in one effect | **missing** | One `animate` per property, each with its own id and events | engine + framework | S–M |
| M5 | `transitionrun/start/end/cancel` events; event detail (property, elapsed) on the `Event` | **missing** | `animation*` events fire; details via `drain_animation_events` | engine + framework | S |
| M6 | Handle object, `finished` future, `reverse()`, `getAnimations()` | **missing** | Poll `animation_state(id)`; negative playback rate for reverse | framework | S |
| M7 | Individual `translate`, `rotate`, `scale` properties | **missing** | Compose in one `transform` list, or `CompositeOperation::Add` | engine | M |
| M8 | `visibility` interpolation (visible for 0 < p < 1 if either end is) | **wrong**: discrete at 0.5 | probe `gap_visibility_is_discrete_at_half_way` | engine | S |
| M9 | `filter` list, `box-shadow`, `clip-path` interpolation | **missing** (only `filter-blur`, `filter-grayscale` numbers; `box-shadow` discrete) | Animate opacity of a shadow layer instead | engine | M each |
| M10 | Custom properties | **missing** (`declaration_block.rs:89-91`) | Typed values in Rust | engine | L |

### Frame scheduling

| # | Capability | State | Evidence / workaround | Owner | Size |
| --- | --- | --- | --- | --- | --- |
| — | Continuous frames while an engine animation runs; idle otherwise | **works** | `desktop.rs:827-829` | — | — |
| M11 | Vsync-paced presentation on every backend; display refresh rate | **missing** | GL: swap interval 1 only; software: unthrottled. See §Measurements | framework (platform) | M |
| M12 | Frame callback (`requestAnimationFrame`) with a frame timestamp, before style | **missing** | `App::on_platform_event` + `Presented` (after present, `App::builder` only) plus a "keep-alive" infinite engine animation to keep frames coming | framework | S–M |
| M13 | Timer resolution | see §Measurements | — | framework | S |
| M14 | Throttle occluded windows | **missing** | Minimized windows already excluded | framework (platform) | S |
| M15 | Public frame/lifecycle statistics | **missing** (`LifecycleStats`, `CompositorStats` exist but are private) | Time the public calls from outside, as `measure_frame_cost` does | framework | S |

### Compositor-driven animation

| # | Capability | State | Evidence / workaround | Owner | Size |
| --- | --- | --- | --- | --- | --- |
| — | Transform/opacity frames skip layout | **works** | `Composite` invalidation; `measure_frame_cost` transform vs width | — | — |
| M16 | Transform/opacity frames skip re-cascade and paint re-record | **missing** | — | engine | M |
| M17 | Damage tracking and partial raster/present | **missing** (full-viewport damage) | Keep windows small; animate small regions | engine + framework | M |
| M18 | Layer promotion (running animation or `will-change`) and cached layer raster | **missing** | — | engine + compositor | L |
| M19 | Off-UI-thread animation (smooth while the UI thread is busy) | **missing** (Div release blocker) | Keep UI-thread work under a frame | engine + framework | L |
| M29 | Starting an animation costs O(running animations) | **slow**: 16 ms per `animate()` with 1000 running (§Measurements) | Start fewer animations; animate a container instead of each child | engine | S–M |
| M30 | Animated fractional `scale()` edge pixels match Chromium | **differs** (static scale is exact) | — | engine (paint) | S–M |

### App-level motion

| # | Capability | State | Evidence / workaround | Owner | Size |
| --- | --- | --- | --- | --- | --- |
| — | Staggered lists | **works** by hand | per-item `delay_ms = index × step` | — | — |
| — | Scroll-linked effects | **works** | native scroll/view timelines | — | — |
| — | Layout reads for FLIP | **works** | `bounding_rect()` flushes layout and is post-transform (probe `transform_animation_moves_the_layout_box_read_by_bounding_rect`) | — | — |
| M20 | Springs/physics, interruptible and velocity-preserving | **missing** | Bake the spring into `linear()` stops and retarget with a new baked curve from the analytic state (RFC §2.3); or integrate per frame (M12 workaround). `transition` retargets from the current value but restarts velocity at zero (probe `interrupting_a_transition_starts_from_the_current_value_without_velocity`) | framework | M |
| M21 | Imperative tweening tied to signals | **missing** | Per-frame `set_property` from `Presented` (measured in §Measurements, `signal` mode) | framework | S (after M12) |
| M22 | Layout animation (FLIP) and shared-element transitions | **missing** helpers | FLIP by hand: rects before, mutate, rects after, additive inverse transform animation | framework | M |
| M23 | Enter/exit presence (animate out before removal) | **missing**: `Show`/`For` unmount at once (`crates/divui/src/renderer.rs`); RFC 0003 deferred exit animations | Keep the node, animate, remove on `animationend` | framework, then components | M + S each |
| M24 | Gesture-driven motion: drag, inertia, snap | **partial**: pointer capture API exists; headless dispatch makes no `pointer*` events (sample-git gap 7); no velocity tracker or decay curve | Coordinate with the interactions sample; decay = baked curve like M20 | framework | M |
| M25 | Element refs and motion directives in `view!` | **missing** (no ref attribute) | `element_by_id` or build elements in code | framework (macros) | S |
| M26 | Lottie / vector animation | **missing**: no stroked or general paths in div-paint | Out of scope until paths land (sample-charts' area) | engine | L |

### Accessibility and correctness

| # | Capability | State | Evidence / workaround | Owner | Size |
| --- | --- | --- | --- | --- | --- |
| — | Reduced-motion behaviour in the engine | **works** (finite animations jump to the end) | probe | — | — |
| M27 | Read the OS reduced-motion setting and its changes | **missing** (no `SPI_GETCLIENTAREAANIMATION`, no portal setting) | App reads the setting itself and calls `set_prefers_reduced_motion` | framework (platform) | S |
| M28 | `@media (prefers-reduced-motion)` | **missing** (no media queries, `docs/status.md:19`) | Typed branches in Rust | engine | M (part of media queries) |

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
  (M17). One animating 24-px square costs as much raster as the whole page.
- **Style sampling grows faster than N**: 100 → 1000 elements is ×17.8
  (transform) and ×16.7 (background-color) in the sample stage. Each frame
  re-cascades every root (M16).
- **Transform frames were not cheaper than width frames** in the update
  stage at N = 1000 (21.75 vs 12.66 ms median) in this layout, although
  transform skips layout: the retained-transform refresh and the full paint
  re-record dominate. At N ≤ 100 they are within 0.3 ms.
- The per-stage split stops at Div's public API: layout and paint recording
  are not separable from outside (M15).

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
frame 18 056 ms after process start. **M29** (engine, S–M): starting an
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
  is no off-thread animation (M19).
- Imperative per-frame motion from the `Presented` callback kept the same
  rate as engine animations at 100 cells, but needs an infinite
  "keep-alive" engine animation to keep frames coming and lands one frame
  late (it runs after present) — M12.

### Timers

`frame_pacing timers 0 6`, no animation (1 frame presented while idle):

| timer | n | mean | median | p95 | p99 | max | min |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `task::time::interval(16 ms)` tick spacing | 186 | 16.02 | 15.66 | 27.82 | 32.12 | 32.41 | 0.00 |
| `task::time::sleep(1 ms)` actual duration | 217 | 13.87 | 15.40 | 16.33 | 16.68 | 17.96 | 1.21 |

`sleep(1 ms)` took 15.4 ms at the median, which is the default Windows
timer tick of about 15.6 ms: timers cannot pace animation on Windows (M13:
a high-resolution waitable timer or `timeBeginPeriod` while animating, and
the frame callback M12 instead of timers for motion).

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
| visibility hidden → visible, linear | 4 / 6 | t = 100, 250: Div hidden, Chromium visible (M8) |

- **M8 confirmed in pixels**: Chromium shows the box for 0 < p < 1, Div
  hides it until p = 0.5.
- **M30 (engine: paint), new**: an *animated* fractional scale rasters
  differently from Chromium at the box edges, while the same static scale
  is exact. The differences are on edge pixels only. Likely cause, not
  verified: Chromium composites an element with a running transform
  animation as its own layer and picks the layer's raster scale from the
  animation, so edge coverage differs from a direct raster. Div already
  imitates Chromium's layered raster for `will-change: transform` paint
  groups (`div-paint/src/painter.rs:1722-1730`); running animations may need
  the same rule. Rotation and translation matched exactly.
- The 1-level differences under `linear()` easing also appear with plain
  linear easing at the same scale, so they belong to M30, not to the easing.

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

None of the portable files exercises Div's animation engine: the porter
freezes all animations at t = 0 (`tools/wpt/port_wpt.py:7657-7664`, and the
Chromium template pauses them, `tools/accountability/run_all_pixel_comparisons.py:69-71`),
and its only sampling path interpolates `background-color`
(`port_wpt.py:3186-3226`). In the WPT tree itself, css-transitions has 6
reftests and 118 testharness files, css-animations 29 reftests and 112
testharness files, css-easing 12 testharness files (excluding support,
references and crash tests).

### WPT: what would carry real signal

WPT's **interpolation tests** are data, not scripts: 327 files under `css/`
(5 tentative) call `interpolation-testcommon.js` with tables of
`{property, from, to}` and `[fraction → expected]` rows — 1 137
`test_interpolation`, 448 `test_no_interpolation` and 531
`test_composition` blocks. The largest groups are css-transforms (32
files), css-backgrounds (30), motion (21), css-masking (21), css-values (18),
css-sizing (16), css-fonts (16), filter-effects (12).

### Plan

1. **Keep the sampled-time probe here** and grow it per property family as
   the gallery needs them (Phase 2), so each workaround has a Chromium check.
2. **Interpolation tables → Div value tests** (engine tools, M): extract the
   tables with a static parser (the porter's approach to HTML), emit one Rust
   test per row that animates the property with `Element::animate` and
   compares the computed value at each fraction to Chromium's expectation;
   for visual properties, also emit a sampled pixel pair. Count the rows that
   map onto Div's typed properties before sizing further.
3. **Sample-at-time in the census harness** (engine tools, M): let the porter
   map the `animation` shorthand and `@keyframes` onto `Element::animate` plus
   `advance_time(t)` instead of freezing at 0, and let the Chromium side use
   negative-delay rewriting (as this probe does) or CDP
   `Animation.seekAnimations` for transitions. Then the 29 + 6 reftests and
   sampled variants of the css-animations files become meaningful.
4. **testharness files** stay out until Div has a script-free way to express
   their assertions; the interpolation tables cover most of what they check
   about values.

## Phase 2: the motion gallery (on go-ahead)

Small PRs, each a page of one window, each workaround marked
`// div-gap: M<n>` and recorded in `docs/framework-gaps.md`:

1. Shell and easing page: every easing curve drawn as a moving dot and a
   plotted curve, sampled headless in tests (snapshot at fixed times).
2. Transitions page: hover/press/toggle states through `Element::transition`
   (M2 workaround), interruption demo.
3. Presence and lists: a toast stack and a filterable list with enter/exit
   and stagger (M23 workaround).
4. Springs and drag: a draggable card with inertia and snap back, spring
   retargeting (M20, M24 workarounds; pointer input coordinated with the
   interactions sample).
5. FLIP: grid re-ordering and a card that expands into a detail view (M22).
6. Scroll-linked: a header that shrinks and a progress bar on scroll and
   view timelines.
7. Stress page: N animating elements with an on-screen frame-time graph
   (M12, M15 workarounds), for repeatable measurements.
