# AutoAnimate port (Phase 2, M6)

`src/auto_animate/` ports AutoAnimate 0.10.2 (`@formkit/auto-animate`,
MIT, "Copyright 2022 FormKit Inc."). `autoAnimate(parent, options)`
animates a parent's children when they are added, removed or moved:

- **Moves** use FLIP.
- **Added children** fade and scale in.
- **Removed children** are put back, absolutely positioned, to animate
  out.

The package's `index.mjs`, `LICENSE` and `package.json` are vendored
unmodified in
[fidelity/web/auto-animate-0.10.2](../fidelity/web/auto-animate-0.10.2),
with their npm integrity and SHA-256 sums in its README.

The port keeps AutoAnimate's formulas and order of operations:

- **Reading a mutation batch** (`getElements`). This includes the
  short-circuit when a removed child is put back, and marking the target
  and all its children.
- **Choosing what each element does** (`animate`): add, remain, remove,
  or nothing for a new element.
- **The measurements:**
  - `getCoords`: `getBoundingClientRect` plus the scroll offset;
  - `getTransitionSizes`: content-box sizes minus padding and borders,
    rounded;
  - `deletePosition`: the removed child's position against its offset
    parent, with margins.
- **The keyframes and options of each `element.animate()` call.**
  - `remain`: FLIP translate, width and height. `deltaTop` is zeroed when
    the bottom did not move.
  - `add`: `scale(.98)` and opacity 0, then the same again at offset 0.5,
    then `scale(1)` and opacity 1, over 1.5 × the duration, `ease-in`.
  - `remove`: `scale(1)` to `scale(.98)` and opacity 1 to 0, `ease-out`.
- **The removal sequence.** The removed child is re-inserted with its
  style reset (nine properties, among them `position: absolute` and
  `z-index`). On the `finish` event, `cleanUp` removes it, and a
  `setTimeout(0)` clears the reset styles.
- **The timers that keep the stored positions current.**
  - `updatePos` is debounced, then waits for `animation.finished`.
  - `poll` starts after a random delay of up to 2 s, then repeats every
    2 s through `requestIdleCallback`.
- **`enable`, `disable` and `destroy`.** `disable` finishes the exit
  animations and cancels the others.
- **The reduced-motion check.** AutoAnimate calls `matchMedia` once, in
  `autoAnimate()`. The port reads Div's `Document::prefers_reduced_motion()`
  instead, the host-set flag that Div has in place of the media query
  (F9).

| File | AutoAnimate source (`index.mjs`) |
| --- | --- |
| `mod.rs` | `autoAnimate`, `getElements`, `animate`, `remain`, `add`, `remove`, `cleanUp`, `deletePosition`, `getCoords`, `getTransitionSizes`, `getScrollOffset`, `isOffscreen`, `updatePos`, `poll`, `handleMutations`, `target`, `getOptions`, `isEnabled`; the controller's `enable`, `disable`, `isEnabled` and `destroy` |
| `platform.rs` | The scaffolds below, and identity maps that stand in for AutoAnimate's `WeakMap`s and `WeakSet`s |

`AutoAnimate` holds AutoAnimate's module state. The host:

- makes its DOM changes through `append_child`, `insert_before` and
  `remove`;
- calls `frame(now)` once per frame;
- calls `checkpoint()` after its own calls in that frame.

The `Event` log records what AutoAnimate did, in the oracle's terms.

Not ported:

- **Plugins**, where the options are a function.
- **The ResizeObserver and IntersectionObserver re-measurements**
  (`observePosition`, `handleResizes`, `updateAllPos`). These need O1 and
  O2, which Div lacks. The comparison pages never resize, and the
  oracle makes both observers inert.
- **`adjustScroll`**, which needs the window's scroll position (F23).
- **`remeasureAfterMove`**, the re-measurement after CSS transitions and
  animations end (F1 and F2).
- **The React, Vue, Solid, Svelte, Angular and Preact wrappers.**

No desktop example is included. The comparisons run headless only.

## The scaffolds

Each scaffold carries a `// div-gap:` comment and is listed in
[framework-gaps.md](framework-gaps.md#scaffolds). All of them run on the
document clock that the host advances, so the headless runs are
deterministic.

- **O3 MutationObserver** (`platform.rs` `MutationRecord`, `Observer`;
  `mod.rs` `append_child`, `insert_before`, `remove`). Div has no
  MutationObserver, so the host's DOM changes go through the port. The
  port makes each change on Div and queues the `childList` records the
  DOM would queue: removal from the old parent, then addition. The
  observer is notified at the next microtask checkpoint.
  - A moved node is moved with Div's `insert_before` or `append_child`,
    never detached first. Div's `Element::detach()` drops the element's
    animations: `animation_state` then reports an unknown animation. A
    move keeps them.
  - The removals that AutoAnimate makes itself (`cleanUp`) go through
    `detach()`, because that element is not reused while it animates.
- **A3 Web Animations API** (`platform.rs` `Animation`). Div's
  `Element::animate` animates one property and returns an
  `AnimationId`. The scaffold stands in for the `Animation` that
  `element.animate()` returns. It holds:
  - one id per property;
  - a `playState`;
  - the `finished` promise's reactions;
  - the `finish` event's listeners.

  It treats the animation as ended when Div reports every property's
  animation finished. Keyframe offsets that AutoAnimate leaves out are
  computed as the Web Animations spec does. The easing is effect-level,
  as in `element.animate()`.
- **F21 HTML timers on the document clock** (`platform.rs` `Timers`).
  `setTimeout` and `setInterval`, run by `frame(now)` in due-time order,
  then id order. Div's `divui::time::sleep` and `interval` run on the wall
  clock, which the headless frame advance does not move.
- **F22 `requestIdleCallback`** (`platform.rs` `IdleTask`). Idle
  callbacks run after the frame's other work.
- **F23 `document.documentElement` and the viewport size** (`mod.rs`
  `AutoAnimate::viewport`). `isOffscreen` compares with
  `documentElement.clientWidth` and `clientHeight`. Div's body has no
  parent element, and `Document` has no getter for the viewport size or
  the window's scroll position (checked at `92ea25a0`). The host passes
  the viewport size to `AutoAnimate::new`.

`frame(now)` runs one frame's work in the order the oracle page runs it:

1. due timers;
2. animations that reached their end, then their `finished` reactions,
   then their `finish` events;
3. idle callbacks.

A microtask checkpoint follows each callback. Then the host's calls run,
and `checkpoint()` runs the mutation observers and the `finish` events of
animations ended by `finish()`.

## Fidelity

Two comparisons against AutoAnimate in pinned Chromium (`chrome-linux64`
147.0.7727.50, Div's pin), both produced by
[`tools/auto_animate_oracle.py`](../tools/auto_animate_oracle.py). The
scenarios are in [tests/auto_animate/cases.json](../tests/auto_animate/cases.json),
and both sides use them.

**How the page runs AutoAnimate.**

- **Loading.** The page loads the vendored `index.mjs` over HTTP from
  127.0.0.1.
- **Time.** The page replaces these before the module loads:
  - `setTimeout`, `setInterval` and their clears: a queue ordered by due
    time, then id;
  - `requestAnimationFrame` and `requestIdleCallback`: queues;
  - `performance.now` and `Date.now`: read the virtual clock;
  - `Math.random`: returns 0.5;
  - ResizeObserver and IntersectionObserver: inert.

  Frame k sets the clock to `k * 1000 / 60`. It then runs, in order:
  1. the due timers;
  2. the animations' ends and `finish` events;
  3. the rAF callbacks;
  4. the idle callbacks;
  5. the case's calls for frame k.
- **Animations.** Each animation is paused at creation and seeked every
  frame. An animation ends when `now/1000 − start/1000 ≥ duration/1000`.
  This is computed in seconds because Chromium's `AnimationTimeDelta` and
  Div's clock use double seconds; computed in milliseconds, `list-options`
  ended one frame early. At the end, the pump calls the native `finish()`
  and waits two native frames so that the `finish` event dispatches. After
  a call to AutoAnimate's own `finish()` (`disable`), it waits the same
  way.
- **Recording.** The page records each frame's events in order:
  - `element.animate()` calls, with their keyframes and options;
  - `cancel()` and `finish()` calls;
  - animation ends;
  - style assignments to the items, through an own accessor on each
    item's style;
  - the DOM operations AutoAnimate makes.
- **Isolation.** Each case gets a fresh page, because AutoAnimate keeps
  module state.
- **Pages.** Each page is 200x120 at device scale 1. The four layouts:
  - `list`: block items with a margin;
  - `row`: a flex row with `flex-shrink: 0`;
  - `padded`: items sized by padding and a border;
  - `boxed`: a padded and bordered parent with bordered items.

  On Div, the page's styles are inline, because Div has no style sheets
  (F8). For the same reason the row items use `flex-shrink: 0`, not the
  `flex` shorthand: Div's runtime parser has no `flex` shorthand.

### Events: 24 cases, 1619 frames, 543 events exact

The 26 cases cover:

- an append, a prepend, a removal (middle, last, the only child), a move;
- a change before AutoAnimate's first `updatePos`;
- an interrupted add and an interrupted move;
- a child put back during its exit, and after it;
- two adds in consecutive frames;
- a batch of a removal, an append and an insert;
- `duration` and `easing` options;
- `disable` and `enable`, `disable` during an exit, and `destroy`;
- reduced motion;
- a long run through the poll interval;
- the row, padded and boxed layouts;
- the two gap cases below.

`cargo test --test auto_animate_values` runs the same cases on the port
on Div elements. Each frame's events must equal the oracle's as JSON.
Div must also accept every style AutoAnimate writes, except
`transform-origin: center` (F24, below).

```sh
python3 tools/auto_animate_oracle.py values DIV_CHECKOUT CHROME_DIR   # WSL
cargo test --test auto_animate_values
```

Result, 15 November 2026:

- **The 24 cases without a gap match in all 1619 frames (543 events).**
- **14 `transform-origin: center` writes were rejected by Div** (F24).
  These are the style reset of removed children. Div's initial
  `transform-origin` is already the centre, and the reset is cleared by
  removing the property, which works.
- **`gap-f8-remove-readd` differs in 1 of 91 frames** (frame 60). A child
  removed at frame 30 is put back at frame 60. `cleanUp` clears the
  child's inline `width`, `height` and `margin` after its exit. In
  Chromium those came from AutoAnimate's reset, and the page's style
  sheet sizes the child again. On Div they were the child's own inline
  styles, so the child comes back with no height. In Chromium the parent
  animates from 44 px to 68 px and the next item moves 24 px; on the port
  neither happens. `padded-remove-readd` puts a child back whose size
  does not depend on those properties, and it matches.
- **`gap-f9-reduced-motion-disrespect` differs in 3 of 61 frames.** The
  case uses reduced motion with `disrespectUserMotionPreference: true`.
  In Chromium the media query has no effect on Web Animations, so the
  animations run. Div's reduced-motion setting makes the engine jump
  every finite animation to its end, so all five animations end at
  frame 31.
- **Recording the oracle a second time gave a byte-identical file.**

### Frames: 43 of 48 exact

These 8 cases were captured at frames 30, 33, 36, 40, 45 and 52:

- `list-append`, `list-prepend`, `list-remove-middle`, `list-move`,
  `list-options`;
- `row-remove-middle`;
- `boxed-append`, `boxed-remove-first`.

- **Chromium:** the pump as above, on the main thread
  (`--disable-threaded-animation`). Each capture follows two of
  Chromium's own frames.
- **Div:** the port on the headless clock (`Document::advance_time`),
  then `render_to_png_buffer()`.

```sh
cargo test --test auto_animate_pixels -- --ignored                   # Div frames (Windows)
python3 tools/auto_animate_oracle.py pixels DIV_CHECKOUT CHROME_DIR   # Chromium, WSL
```

Result, 15 November 2026, with the Div frames from Windows (`motion-env`,
the CI Skia flags) and Chromium in WSL. Two runs each gave **43 of 48
frames exact**, with the same differences. The Chromium PNGs were
byte-identical between the runs.

| Case | Frame | Differing pixels | Max channel delta |
| --- | ---: | ---: | ---: |
| boxed-append | 45 | 2078 | 4 |
| boxed-append | 52 | 584 | 33 |
| boxed-remove-first | 33 | 2500 | 62 |
| boxed-remove-first | 36 | 1379 | 48 |
| boxed-remove-first | 40 | 2594 | 46 |

All five frames show bordered items of odd size under `scale(.98)` with
opacity below 1. Interior colours differ by about 1, and the edges by
more. The cause is not diagnosed. It may be the animated fractional
`scale()` raster difference recorded under F6 (A4), but that is not
shown. The `list` and `row` frames, whose items have no border, are all
exact, including under the same scale and opacity.

These results cover only these cases and frames. They do not cover:

- the unported parts above;
- pages that resize or scroll;
- other layouts;
- Div's frame scheduling, which does not exist yet (R1).

## Gaps found

New, none yet in Div's plan:

- **F21 HTML timers on the document's event loop and clock.**
  `setTimeout` and `setInterval` that the headless frame advance runs, at
  the document's time.
  - Owner: framework.
  - Size: S–M.
  - Scaffolded as above. Every case measures it.
- **F22 `requestIdleCallback`.**
  - Owner: framework.
  - Size: S.
  - Scaffolded as above. Not measured: the idle `updatePos` only
    refreshes stored positions, and no case's events depend on it.
    `list-long-poll` runs to frame 180, when the poll interval first
    fires at 3000 ms.
- **F23 `document.documentElement` and the viewport's client size.**
  Div's document has no root element above the body, and no getter for
  the viewport size or the window's scroll position. The window scroll
  gap was only checked to this extent.
  - Owner: framework.
  - Size: S–M.
  - Scaffolded as above. `adjustScroll` is not ported because of it.
- **F24 `transform-origin` in Div's runtime parser.**
  `StyleProperty::TransformOrigin` exists, but `parse_literal` has no
  case for it (`crates/div-style/src/property.rs:1986-2210`, at
  `92ea25a0`).
  - Owner: engine (style).
  - Size: S.
  - Counted by `tests/auto_animate_values.rs` (14 writes). No visible
    effect in these cases.

Existing, with new evidence:

- **F8 selectors and cascade** (S1): `gap-f8-remove-readd`, above.
- **F9 media queries and reduced motion** (S4): Div's reduced-motion
  setting ends script-created animations, which the web does not do;
  `gap-f9-reduced-motion-disrespect`, above.
- **F3 Web Animations API** (A3): the scaffold, and `detach()` dropping
  animations.
- **F11 observers** (O1, O2, O3): MutationObserver is scaffolded.
  Resize and intersection re-measurement is not ported.

A7 (retaining removed children) was planned for M6 but is not needed by
this port. AutoAnimate puts removed children back itself, on the DOM. A7
applies only to a renderer that unmounts children, such as a framework
wrapper. No wrapper is ported.
