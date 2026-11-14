# Framework gaps

The web platform features Div is missing for the motion packages are in
[design.md](design.md#platform-features-what-div-has-what-is-missing). Each
has a number `F<n>`, an owner and a size. Div should implement those
features. This repository's ports use only Div's public surface.

A workaround here is temporary scaffolding that unblocks building a port.
It is never the proposed solution. Each one:

- carries a `div-gap: <plan ID> <web feature>` comment naming the platform
  feature that will replace it, for example
  `// div-gap: R1 requestAnimationFrame`, so `git grep -n "div-gap: R1"`
  finds every use;
- has an entry below: what the package needs, what was tried, what
  happened, the scaffold used, and how bad it is;
- is deleted when Div ships the feature.

Pinned Div: `92ea25a0` (Phase 1 was measured at `565db46a`; the only change
between them is Div #30, view children, which touches no animation code).

## Plan IDs

Scaffolds use the IDs of Div's consolidated platform plan, which the owning
tracks schedule against. The design's feature numbers map to them as
follows. "Tier 0" items are scheduled in Div; the others are not yet.

| Design | Plan ID | Web feature | Owner | Size | Tier |
| --- | --- | --- | --- | --- | --- |
| F1 | A1 | CSS Transitions from style rules | engine | L | 1 |
| F2 | A2 | CSS Animations, `@keyframes`, events with detail | engine | L | 1 |
| F3 | A3 | Web Animations API surface | framework + engine | M | 1 |
| F4 | R1 | `requestAnimationFrame` (with T1 frame advance) | framework | M | **0** |
| F5 | R2 | Frame pacing and high-resolution timers | framework (platform) | M | 1 |
| F6 | A4 | Interpolation coverage and raster parity (`visibility`, animated `scale()`) | engine | S / M each | 1 (visibility) |
| F7 | S3 | Custom properties | engine | L | **0** |
| F8 | S1 | Selectors and cascade | engine | L | **0** |
| F9 | S4 | Media queries, `matchMedia`, OS reduced motion | engine + framework | M + S | **0** (MQ), 1 (OS setting) |
| F11 | O1, O2, O3 | Resize, Intersection, Mutation observers | engine + framework | M each | **0** (O1), 1 (O2, O3) |
| F12 | I1 | Pointer Events | framework | M | **0** |
| F13 | A5 | Scroll-driven animation syntax and objects | engine + framework | M | 3 |
| F14 | A6 | View Transitions | engine + compositor | L | 3 |
| F15 | R4, R5, R6 | Damage and partial raster; targeted sampling and O(1) start; compositor layers | engine (+ compositor) | M + S–M; L | **0** (R4, R5), 3 (R6) |
| F16 | V1 | Inline SVG | engine | L | 1 (starts right after the engine's Tier 0, Q9) |
| F17 | C2 | Canvas 2D | engine | L | 2 |
| F18 | A7 | Retaining removed keyed children until exit finishes (non-web contract, accepted by Q15) | framework | S–M | 3 |
| F19 | none yet | CSSOM inline style serialization (`el.style.x`, `getPropertyValue` as strings) | engine (style) + framework (API) | M | not planned |
| F20 | none yet | Transform functions in Div's parser (`skew*`, `rotateX/Y/Z`, `translateZ`, 3D, `perspective`) | engine (style, paint) | M | not planned |

F10 (geometry and computed style) already works and has no plan ID. F19
and F20 were found in M5 (anime.js) and are not yet in Div's plan.

The user's platform decisions of 13 November 2026 (`decisions-2026-11-13.md`)
apply to these entries:

- **R4/R6 (Q2).** One partial-redraw design, damage then layers, owned by
  the engine. Animation layers and View Transitions build on it.
- **A3 (Q12).** One Web Animations surface; CSS-created animations appear
  in `getAnimations()`. Every `div-gap: A3` scaffold here uses
  `Element::animate → AnimationId`, which is retired after migration, so
  each scaffold is replaced by the new surface rather than kept as a
  wrapper.
- **S3 (Q3).** Runtime custom-property writes are supported, and
  third-party sheets go through browser-compatible ingestion.
- **A7 (Q15).** Accepted as framework work.

## Scaffolds

| # | Scaffold | Replaced by | Package | Status |
| --- | --- | --- | --- | --- |
| 1 | `src/motion/mod.rs` `animate`: `div-gap: A3 Web Animations API`. Motion's `element.animate()` call goes to `Element::animate` per property; the `AnimationId` stands in for the `Animation`. Motion's `onfinish` (commit the final style, cancel) does not run, so the filling animation stays and renders the same final value. A single keyframe (implicit from-value) is not expressible and returns an error. | A3 Web Animations surface (Q12) | Motion 14.14.0 | in use. M3: 52 of 52 cases' calls equal Motion's, 90 of 90 frames equal Chromium's. M3b (`stagger`, sequences, through `animate_staggered` and `animate_sequence`): 37 of 37 cases, 111 calls equal, 128 of 128 frames equal ([motion-port.md](motion-port.md)) |
| 2 | `src/react_spring/rafz.rs` `start` and `src/react_spring/frame.rs`: `div-gap: R1 requestAnimationFrame`. rafz's `nativeRaf(loop)` is queued in the `Runtime`, and the host runs it with `Runtime::animation_frame(now)`, with a microtask checkpoint after each callback. Headless: `Document::advance_time(t)`, then the frame at the clock's time. Desktop: frames from `PlatformEvent::Presented` (after present, so a written value shows one frame late), kept coming by an infinite 1 px opacity animation (`keep_alive`). | R1 `requestAnimationFrame` (Tier 0) | react-spring 10.1.2 | in use. M4: 49 cases, 5249 frames of values bit-identical to react-spring in Chromium; 143 of 143 frames exact ([react-spring-port.md](react-spring-port.md)). The desktop path runs (`examples/react_spring_toggle.rs`) but is not measured. |
| 3 | `src/anime/engine.rs` `Runtime::animation_frame` and `src/anime/frame.rs`: `div-gap: R1 requestAnimationFrame`. anime.js's `requestAnimationFrame(tickEngine)` is queued in the `Runtime`; the host runs it with `animation_frame(now)`, and `Date.now()` reads the runtime's time. Headless: `Document::advance_time(t)`, then the frame at the clock's time in whole milliseconds. No desktop example. | R1 `requestAnimationFrame` (Tier 0) | anime.js 4.5.0 | in use. M5: 46 cases, 2670 frames, 3737 style writes equal to anime.js's in Chromium; 66 of 66 frames exact ([anime-port.md](anime-port.md)) |
| 4 | `src/anime/engine.rs` `Target::inline` and `src/anime/dom.rs` `DivHost::computed_value`: `div-gap: F19 CSSOM inline style serialization`. When an animation starts from the current value, anime.js reads `el.style.transform`/`el.style.opacity` back. The port reads back the string it wrote, and formats Div's typed computed opacity with Rust's `f32` formatting. Chromium serializes to six significant digits. | F19 CSSOM inline style serialization (no plan ID yet) | anime.js 4.5.0 | in use. Differs: `gap-f19-readback` reads `translateX(171.2925px)` where Chromium reads `translateX(171.292px)`; 22 of 51 frames differ ([anime-port.md](anime-port.md#the-scaffolds)) |

Not scaffolded: values Motion runs on its own frame loop (inertia, `repeatDelay`, `repeatType: "mirror"`, `damping: 0`, non-accelerated values). The port returns `NotNative::MainThread` for them; they wait for R1.

Not portable yet: Motion's native `filter` and `clipPath`. Div has only the `filter-blur`, `filter-grayscale` and `clip-path-inset` longhands, not CSS `filter` function lists or `clip-path` shapes. This is A4 (F6), engine, M each. No scaffold: a private filter model would reimplement the platform feature.

Not portable yet: anime.js's transforms other than `translateX/Y`, `rotate`
and `scale*` with Div's parser. This is F20, engine, M. `DivHost` reports
such writes as errors; no case animates them.
