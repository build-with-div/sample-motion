# RFC draft: motion for Div

- Status: draft from the sample-motion investigation, for the Div tracks to
  adopt, change or reject. Nothing here is scheduled.
- Pinned Div: `565db46a`. Evidence: [design.md](design.md) and the probes in
  `tests/` and `examples/` of this repository.
- Owners named per part: **engine** (div-style, div-layout, div-paint,
  div-engine), **framework** (divui, div-platform, div-macros), **components**
  (div-components). Sizes: S (days), M (one to two weeks), L (more).

## Summary

Div already has a capable animation *engine*: typed keyframe effects with
Chromium's timing model, every CSS easing function, composition, controls,
scroll and view timelines, and a deterministic manual clock. What it lacks
is everything around it that an app author touches: CSS syntax, a frame
callback, a handle type, presence, springs, frame pacing and a cheap path for
compositor-only properties.

This RFC proposes, in order of value per effort:

1. **Frame and clock plumbing** (framework): a frame callback, real frame
   timestamps, vsync-paced presentation on every backend, OS reduced-motion.
2. **`divui::motion`** (framework): an `Animation` handle, named easings,
   `transition:` in `view!`, springs baked into `linear()` easings,
   `Presence`, stagger and a FLIP helper — all on top of the existing engine.
3. **CSS-level animation** (engine): `transition-*`, `animation-*` and
   `@keyframes` through the typed-style-rules work (RFC 0001), individual
   transform properties, correct `visibility` interpolation, transition events.
4. **Compositor-only frames** (engine + framework): transform/opacity frames
   that skip the re-cascade, the paint re-record and the full raster.
5. **Fidelity** (engine tools): sample-at-time comparison with Chromium and a
   port of WPT's data-driven interpolation tests.

## 1. Frame and clock plumbing (framework)

### 1.1 `on_animation_frame`

```rust
/// Run `callback` before style for each frame while the returned guard lives.
/// Registering keeps the window producing frames, like an animation does.
pub fn on_animation_frame(callback: impl FnMut(&FrameInfo) + 'static) -> FrameCallback;

pub struct FrameInfo {
    /// The document clock for this frame (what `begin_frame` receives).
    pub time_ms: f64,
    /// Time since the previous frame of this window.
    pub delta_ms: f64,
    /// The estimated time the frame will be on screen, when the backend knows it.
    pub target_present_ms: Option<f64>,
}
```

- Runs inside `begin_frame`, after pending events and before
  `advance_time`, so style written in the callback lands in the same frame
  (today `PlatformEvent::Presented` runs after present, one frame late).
- Available from `AppSpec` windows, not only `App::builder()`.
- Headless: `HeadlessApp::render_at(t)` runs the callbacks with `time_ms = t`,
  so frame-driven motion is as testable as engine animations.
- Dropping the `FrameCallback` unregisters it; scopes own it like effects.

Size **S–M**. Removes the "keep-alive animation" workaround (design gap M12).

### 1.2 Frame pacing and timestamps (platform)

- Only the GL path asks for a swap interval of 1; software presentation has
  no throttle (the framework track recorded ~690 frames/s for one idle
  window). With 100 animated cells both ran CPU-bound at 64–68 frames/s on a
  239 Hz display (design §Measurements). Pace it:
  `DwmFlush` or a waitable swap chain on Windows, `wl_surface.frame`
  callbacks on Wayland (call `pre_present_notify`), and vsync on X11.
- Use the display's refresh period, not 60 Hz, and expose it in `FrameInfo`.
- Stamp frames with the expected present time rather than the redraw start.
- `task::time::sleep(1 ms)` took 15.4 ms at the median on Windows (design
  §Timers): use a high-resolution waitable timer for UI timers, and drive
  motion from frames (1.1), never from timers.
- Pause frame production for occluded windows (`WindowEvent::Occluded`), as
  for minimized ones; keep the document clock on wall time so animations
  resume at the right place (CSS semantics).

Size **M**.

### 1.3 Reduced motion from the OS

- Initialise `Document::set_prefers_reduced_motion` from
  `SPI_GETCLIENTAREAANIMATION` (Windows) and the `org.freedesktop.appearance`
  / GNOME `enable-animations` setting (Linux portal), and update it on
  `WM_SETTINGCHANGE` / portal change signals.
- Expose it as a signal: `div::motion::reduced_motion() -> Memo<bool>`.
- The engine's current behaviour (finite animations jump to the end) stays
  the default; `motion` helpers below pick shorter cross-fades instead when
  the app opts in.
- `@media (prefers-reduced-motion)` waits for media queries (engine).

Size **S** (framework) plus the media-query work (engine, not sized here).

### 1.4 Frame statistics

`Document::lifecycle_stats()` (layouts, paints, scenes) and
`App::frame_stats()` (presented, dropped, interval histogram) as public
read-only values, so apps and samples can measure without timing the API
from outside. Size **S**.

## 2. `divui::motion` (framework)

Everything here builds on `Element::animate` / `transition`, so it inherits
the engine's Chromium-matched timing and the deterministic clock.

### 2.1 `Animation` handle and named easings

```rust
let fade = element.animate(StyleProperty::Opacity, Keyframes::from_values(0.0, 1.0),
    AnimationOptions { duration_ms: 200.0, easing: Easing::EASE_OUT, ..Default::default() })?;
fade.pause()?; fade.seek(50.0)?; fade.reverse()?;
fade.finished().await;            // resolves on end, errors on cancel
let all = element.animations()?;  // getAnimations()
```

- `Animation` wraps `AnimationId` + `WeakDocument`; existing `Document`
  methods stay.
- `Easing::{EASE, EASE_IN, EASE_OUT, EASE_IN_OUT}` constants (engine,
  trivial).
- `animate_keyframes` for several properties in one effect, with per-frame
  offsets — today each property is its own animation and its own events.
- Animation events carry `AnimationEventInfo { property, elapsed_time_ms,
  iteration }` on the `Event` instead of only through
  `drain_animation_events`.

Size **S**.

### 2.2 `transition:` in `view!`

Until the engine has CSS transitions, the macro can lower a declared
transition onto `Element::transition` for dynamic styles:

```rust
view! {
    <div
        style:opacity=move || if open.get() { 1.0 } else { 0.0 }
        transition:opacity=Transition::new(150.0).easing(Easing::EASE_OUT)
    />
}
```

When the reactive value changes, the generated effect calls
`element.transition(property, value, options)` instead of `set_property`.
Interrupting restarts from the current computed value (what the probe
`interrupting_a_transition_starts_from_the_current_value_without_velocity`
shows the engine does). Size **S–M** (macros).

### 2.3 Springs baked into `linear()`

A damped spring has a closed form. Sampling it into `Easing::LinearStops`
gives an engine-driven animation (no per-frame callback, deterministic,
compositable later) that matches what browsers render for the same
`linear()` easing:

```rust
let x = spring(0.0, Spring { stiffness: 300.0, damping: 30.0, mass: 1.0 });
view! { <div style:transform=move || translate_x(x.get()) /> }
x.set_target(240.0);   // on drag end: x.set_target_with_velocity(240.0, v)
```

- `set_target` reads the current value and velocity from the running
  solution (analytic, not sampled from pixels), solves a new spring from
  that state, bakes it to stops (adaptive: error < 0.5 px or < 0.001
  opacity) and starts a transition with that easing. Velocity is preserved
  across interruptions, which plain `transition` cannot do.
- The reactive value `x.get()` follows the analytic solution on frames, so
  logic that reads it agrees with what is drawn.
- `Decay { velocity, rate }` (inertia) and `snap_to(points)` use the same
  path; they are the pieces gesture-driven motion needs (coordinate with
  the interactions sample on pointer capture and velocity tracking).

Size **M**. Fallback without the engine path: integrate per frame with
`on_animation_frame` (1.1).

### 2.4 `Presence` (enter/exit)

```rust
view! {
    <Presence when=move || open.get()
              enter=Fade::in_(150.0) exit=Fade::out(120.0).then(Scale::to(0.96))>
        <Dialog/>
    </Presence>
}
```

- On `when` turning false, `Presence` keeps the subtree mounted, marks it
  `inert`, runs the exit animations and unmounts on their `animationend`
  (or immediately under reduced motion).
- Re-entering during an exit reverses the running animations
  (`set_animation_playback_rate(-1)`) instead of remounting.
- `For` gets the same through `enter`/`exit`/`stagger` props:
  `<For each=items key=|i| i.id enter=Slide::up(12.0) stagger=30.0 …/>`,
  where `stagger` sets `delay_ms = index * stagger` (works today by hand).
- RFC 0003 deferred exit animations "until animation events from typed
  rules"; this design needs only the existing `animationend`.

Size **M** (framework), then **S** per component (components: Dialog,
Popover, Tabs indicator, Toast).

### 2.5 Layout animation (FLIP)

```rust
layout_transition(&[&list], Transition::new(200.0), || items.update(|v| v.shuffle()));
```

1. Record `bounding_rect()` of the tracked elements.
2. Run the mutation; flush layout (reading `bounding_rect()` already does).
3. For each element that moved, start a transform animation from the
   inverse delta to identity (additive, so it composes with app transforms).

Shared-element transitions add `layout_id="card-3"`: the helper matches the
outgoing and incoming element by id across a `Show`/`For` swap, which needs
`Presence` (2.4) to keep the outgoing one alive for the first frame.
Size **M**.

## 3. CSS-level animation (engine)

| Item | Why | Size |
| --- | --- | --- |
| `transition-*`, `animation-*`, `@keyframes` in typed style rules (RFC 0001 already orders them in the cascade) | Authors and the WPT porter speak CSS; automatic transitions on style change | L |
| `transitionrun/start/end/cancel` events, with `propertyName` | Transitions today fire `animation*` events | S |
| Individual `translate`, `rotate`, `scale` properties | Independent motion of the same element without list matching | M |
| `visibility` interpolation (visible if either end is, for 0 < p < 1) | Probe `gap_visibility_is_discrete_at_half_way`; fade-out-then-hide patterns | S |
| `filter` (general list), `box-shadow`, `clip-path` interpolation | Common in professional UI motion (shadows on lift, reveal clips) | M each |
| Custom properties and `@property` | Theme-token animation; blocked on var() support | L |
| Start animations without re-cascading every root (or batch starts per frame) | Starting the 1000th animation costs 16 ms; 1000 staggered items take 16 s (design M29) | S–M |
| Raster parity for animated fractional `scale()` | Edge pixels differ from Chromium while the static scale is exact (design M30) | S–M |

## 4. Compositor-only frames (engine + framework)

Today a transform/opacity frame skips layout but still re-cascades every
root, re-records the whole paint, rasters the full viewport on the CPU and
uploads the full frame (design §Compositor). Proposal, in steps that each
pay off alone:

1. **Targeted style:** sample effects into the computed style of only the
   animated elements instead of re-cascading every root. (engine, M)
2. **Damage:** report the union of old and new visual rects of changed
   elements instead of the full viewport, and raster only that region;
   present with damage (softbuffer `present_with_damage`,
   `eglSwapBuffersWithDamage`). (engine + platform, M)
3. **Layers:** promote elements with a running transform/opacity animation
   (and an authored `will-change`) to retained raster layers; a frame then
   re-composites cached layers with new matrices/alpha. This is what the
   existing `RetainedSceneSnapshot` metadata and the unused `SceneMailbox`
   were laid out for. (engine + compositor, L)
4. **Off-thread curves:** hand promoted layers' curves to a compositor
   thread so a blocked UI thread does not stall them (the existing
   threading-model release blocker). (L)

The numbers to beat for steps 1–2 are in design §Measurements: at
1280 × 800 a frame with one animated square costs about 7 ms, nearly all
full-viewport raster.

## 5. Fidelity (engine tools)

- **Sample at time t.** Chromium side: CSS-only, `animation-delay: -t` plus
  `animation-play-state: paused` (already the census' freeze rule), or CDP
  `Animation.seekAnimations` for transitions. Div side: the porter maps
  `animation` / `@keyframes` onto `Element::animate` and calls
  `advance_time(t)`. Today the porter freezes everything at t = 0 and only
  interpolates `background-color` (`tools/wpt/port_wpt.py:3186`).
- **Interpolation tests.** WPT's `*-interpolation*.html` files are tables
  of `{property, from, to, [fraction → expected]}` run by
  `interpolation-testcommon.js`. They need no JavaScript execution to port:
  extract the tables, generate a Div value test per row (computed value at
  fraction p) and, for visual properties, a Chromium sampled pixel pair. See
  design §Fidelity for counts.
- **css-animations / css-transitions suites.** The coverage porter accepts
  86 of 238 css-animations files (21 reference files, 53 `-manual` files,
  12 others, all frozen at t = 0) and 5 of 186 css-transitions files; the
  rest are rejected as `uses_javascript`. They become meaningful only after
  the sample-at-time work.

## Out of scope

Lottie and vector animation need stroked paths and path morphing in
div-paint first (sample-charts' area). 3D transforms and `perspective` are
tracked by the engine track's transform work.

## Open questions

1. Should springs be engine-native easing (`Easing::Spring`) so CSS
   `linear()` export and compositor curves share one representation, or stay
   framework-baked stops?
2. Does `on_animation_frame` run per window or per document, and with which
   clock when several windows have different refresh rates?
3. Should `Presence` own `inert`/focus handling, or leave it to Dialog?
