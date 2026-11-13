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
| F16 | V1 | Inline SVG | engine | L | 1 |
| F17 | C2 | Canvas 2D | engine | L | 2 |
| F18 | A7 | Retaining removed keyed children until exit finishes | framework | S–M | 3 |

F10 (geometry and computed style) already works and has no plan ID.

## Scaffolds

| # | Scaffold | Replaced by | Package | Status |
| --- | --- | --- | --- | --- |
