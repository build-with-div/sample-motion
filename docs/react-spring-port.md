# react-spring port (Phase 2, M4)

`src/react_spring/` ports react-spring's core animation engine: the
`SpringValue` of `@react-spring/core` 10.1.2 for numbers, the frame loop of
`@react-spring/shared` 10.1.2, and the frame scheduler `@react-spring/rafz`
10.1.2. The packages are vendored unmodified in
[fidelity/web/react-spring-10.1.2](../fidelity/web/react-spring-10.1.2).
The port keeps react-spring's formulas and its order of operations: the
1 ms Euler steps of the spring, the duration and decay branches, how
`_merge` decides to start, keep velocity or reset, rafz's frame `dt`, and
when a chain's next step starts.

| Module | react-spring source |
| --- | --- |
| `spring.rs` | `SpringValue` (`advance`, `_update`, `_prepareNode`, `_merge`, `_set`, `_start`, `_stop`, `stop`), `scheduleProps`, `runAsync` for `to` arrays, `AnimatedValue` (`setValue`, `reset`) |
| `config.rs` | `config` presets, `AnimationConfig`, `mergeConfig`, `sanitizeConfig`, the easings built from arithmetic and `Math.pow` |
| `rafz.rs` | rafz's update queue, `setTimeout`, `start`, and the `requestAnimationFrame` it calls |
| `mod.rs` | `Runtime`: rafz's `loop` and `update`, `frameLoop.start` and `frameLoop.advance`, the microtask queue for chains, and the public API |
| `dom.rs` | writing a value to inline `opacity` or `transform`, as `animated` does on each change |
| `frame.rs` | the R1 scaffolds: headless frames and the desktop frame source |

`motion::js::math_exp` is V8's `Math.exp`, which the decay branch needs
(see [Values](#values-5249-of-5249-exact)).

## What it covers

Props: `to` (a number, or an array of updates run as a chain), `from`,
`config` (`tension`, `friction`, `mass`, `damping`, `frequency`,
`velocity`, `precision`, `restVelocity`, `clamp`, `bounce`, `duration`,
`easing`, `progress`, `decay`, `round`, and the presets), `delay`,
`immediate`, `reset` and `default`. Methods: `new SpringValue(props)`,
`start`, `stop`, `get`, `velocity`, `isAnimating`, `idle`.

Interruption keeps velocity as react-spring does. A new target while the
node is still moving keeps `lastVelocity`, because `AnimatedValue.reset()`
clears it only for a finished node. `config.velocity` resets to 0 on each
update with `to` or `from`, unless the update sets it.

Two of rafz's behaviours are ported too, and the cases test them:

- **The frame time step.** The first frame after the loop starts gets a
  `dt` of 16.667 ms. Later frames get `min(64, ts - prevTs)`.
- **The second loop.** rafz queues its next `requestAnimationFrame`
  before it checks whether work is left. A spring started on the frame
  where the loop stopped therefore queues a second `loop`. Both run on
  every later frame, and the second runs with `dt` 0
  (`restart-as-loop-stops`).

Not ported:

- strings, colours and arrays;
- `FluidValue` goals (a spring following another spring);
- `loop`, `reverse`, `pause`, `cancel`;
- async `to` functions and chains inside chains (`start` returns
  `Unsupported`);
- the event props (`onStart`, `onChange`, `onRest`, ...);
- the easings that use `Math.sin`, `Math.cos`, `Math.sqrt` or
  `Math.pow(2, ...)` (sine, circ, expo, elastic, bounce, `steps`);
- the React layer: `Controller`, `useSpring`, `useTransition` and the
  `animated` components.

## The R1 scaffolds

react-spring runs from `requestAnimationFrame`, which Div does not have
(R1, Tier 0). The port keeps rafz's own `nativeRaf(loop)` call. The
callback is queued in the `Runtime`, and the host runs it with
`Runtime::animation_frame(now)`. HTML runs the animation frame callbacks
with a microtask checkpoint after each, so `animation_frame` does the same,
and a chain's next step starts there.

Two hosts, both marked `div-gap: R1` and listed in
[framework-gaps.md](framework-gaps.md):

- **Headless** (`frame::headless_frame`): `Document::advance_time(t)`,
  then `animation_frame` at the document clock's time. The tests drive
  frames this way.
- **Desktop** (`frame::presented_time`, `frame::keep_alive`): frames come
  from `PlatformEvent::Presented`. An infinite engine animation on a 1 px
  element keeps frames coming. Presented runs after present, so a value
  written from it shows one frame late (measured in Phase 1,
  [design.md](design.md)).
  - `examples/react_spring_toggle.rs` uses this. A box springs between
    two positions, retargeted every 700 ms.
  - Run on Windows on 13 November 2026 with
    `cargo run --example react_spring_toggle -- 3`, it exited on its own
    after 3.7 s. That exit needs Presented frames to keep arriving. The
    motion was not inspected or measured.

When R1 lands, `animation_frame` becomes the callback Div calls, and
`frame.rs` is deleted.

`bounding_rect`, listed for M4 in the plan, is not used. `SpringValue`
does not measure layout. react-spring's `useMeasure`-style uses and
@use-gesture come with the React layer.

## Fidelity

Two comparisons against react-spring running in pinned Chromium
(`chrome-linux64` 147.0.7727.50, Div's pin), both produced by
[`tools/react_spring_oracle.py`](../tools/react_spring_oracle.py). The
scenarios are [tests/react_spring/cases.json](../tests/react_spring/cases.json),
shared by both sides.

**How the page runs react-spring.**

- **Loading.** The page loads the vendored ES modules through an import
  map. It is served over HTTP from 127.0.0.1, because Chromium does not
  load modules from `file://`. `react` maps to a stub, since `SpringValue`
  touches React only through one `React.createContext` call at load.
- **Time.** Time is the rAF pump that
  [q13-chromium-capture.md](q13-chromium-capture.md) measured as
  byte-stable. `requestAnimationFrame` and `performance.now` are replaced
  before react-spring loads. Frame k runs the queued callbacks at
  `k * 1000 / fps` ms, with a macrotask turn after each, so chained
  updates continue between callbacks as in a browser frame.
- **Isolation.** Each case gets a fresh page, because rafz and the frame
  loop are module state.

### Values: 5249 of 5249 exact

49 cases, for 5249 frames in all:

- the presets, and `tension`/`friction`/`mass`;
- `frequency`/`damping`, `clamp` and `bounce`;
- `precision`/`restVelocity`, initial velocity and `round`;
- `duration` with every ported easing, `progress`, and a duration
  changed midway;
- `decay`, `delay`, `immediate`, and `reset` (from `from` and explicit);
- interruptions, between springs and between durations and springs;
- `stop` and restart;
- a default config from the constructor;
- two springs;
- chains: inherited config, step delays, a delayed chain, a stopped chain,
  a replaced chain, no-op steps;
- 30, 10 and 144 frames per second. At 10 fps, `dt` is clamped to 64 ms.

After each frame's callbacks and that frame's actions, the oracle records
`spring.get()`, `spring.velocity` and `spring.isAnimating` for each spring.
`cargo test --test react_spring_values` runs the same cases on the port
and requires every number to be bit-identical.

```sh
python3 tools/react_spring_oracle.py values DIV_CHECKOUT CHROME_DIR   # WSL
cargo test --test react_spring_values
```

Result, 13 November 2026:

- **All 5249 frames are bit-identical.** Recording the oracle a second
  time gave a byte-identical file.
- **The first port run differed in 24 frames.** All 24 were in the two
  decay cases, and all were last-bit differences. The port called Rust's
  `f64::exp`, which uses the platform's libm. V8's `Math.exp` is fdlibm's,
  with `exp(1)` special-cased to return E. The port now uses that function
  (`motion::js::math_exp`), checked against V8's output in a unit test.

### Frames: 143 of 143 exact

13 cases, at frames 0, 1, 3, 6, 10, 15, 20, 30, 45, 60 and 90:

- springs: default, wobbly, bounce, opacity;
- durations with easings, decay, delay;
- interruptions: one reversal, frequent retargeting, two springs;
- two chains, and the restart that creates rafz's second loop.

The page is 200×120 at device scale 1, with a 60 px box. After every
frame, each spring's value is written to the box's inline style, as
`translate(<x>px, 0px)` or `opacity`.

- **Chromium:** the pump as above, on the main thread
  (`--disable-threaded-animation`). Each capture follows two of
  Chromium's own frames, through its `requestAnimationFrame` saved before
  the pump replaced it.
- **Div:** the port with `frame::headless_frame` on the document clock,
  then `StyleTarget::write` and `render_to_png_buffer()`. The test checks
  that the clock reads back the exact frame time.

```sh
cargo test --test react_spring_pixels -- --ignored            # Div frames (Windows)
python3 tools/react_spring_oracle.py pixels DIV_CHECKOUT CHROME_DIR   # Chromium, WSL
```

Result, 13 November 2026, with Div frames from Windows (`motion-env`, the
CI Skia flags) and Chromium in WSL:

| Run | Chromium capture | Exact frames |
| --- | --- | --- |
| 1 | style write, 50 ms wait, capture | 143 of 143 |
| 2 | the same | 142 of 143: `interrupt-retarget-often` frame 30, 120 pixels, max channel delta 2 |
| 3 | style write, two Chromium frames, capture | 143 of 143 |
| 4 | the same | 143 of 143 |

Div's frames are deterministic, so the run 2 difference was on the
Chromium side: a capture of a frame that did not yet hold the new style.
The tool now waits for two of Chromium's frames, the same settle as
`tools/motion_oracle.py`.

These results cover only these cases and frames. They do not cover
string or colour values, the React layer, or Div's frame scheduling, which
does not exist yet (R1).
