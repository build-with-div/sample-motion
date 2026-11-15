#!/usr/bin/env python3
"""Run AutoAnimate 0.10.2 in pinned Chromium: the oracle for the Rust port.

Run in the Linux/WSL environment that holds Div's pinned Chromium:

    python3 tools/auto_animate_oracle.py values DIV_CHECKOUT CHROME_DIR
    python3 tools/auto_animate_oracle.py pixels DIV_CHECKOUT CHROME_DIR

The page loads the vendored, unmodified module
fidelity/web/auto-animate-0.10.2/index.mjs over a local HTTP server.

Everything AutoAnimate schedules runs on one virtual clock, replaced
before the module loads:

- `setTimeout`, `setInterval` and their clears are a queue ordered by due
  time, then by id;
- `requestAnimationFrame` and `requestIdleCallback` are queues;
- `performance.now()` and `Date.now()` read the clock, and
  `Math.random()` returns 0.5 (AutoAnimate's poll offset);
- every `element.animate()` animation is paused at creation and seeked to
  the clock's time since its creation each frame, computed in seconds as
  Chromium's default `AnimationTimeDelta` computes an animation's current
  time from timeline times. When that reaches its duration (in seconds) it
  is finished with `finish()`, and the page waits for two of
  Chromium's own frames so that the `finish` event is dispatched;
- `ResizeObserver` and `IntersectionObserver` are inert: Div has neither
  (plan O1, O2), so the port has neither, and their re-measurements are
  not compared.

Frame k, at k * 1000 / 60 ms, runs the due timers, then the animations,
then the queued animation frame and idle callbacks, each followed by a
macrotask turn, then the case's calls for frame k. Mutation observer
callbacks run at their microtask checkpoints, as Chromium runs them.

`values` runs each case of tests/auto_animate/cases.json in a fresh page
(AutoAnimate keeps module state) and records, per frame, what AutoAnimate
does: each `element.animate(keyframes, options)` call, `cancel()` and
`finish()` call, each animation's end, each assignment to the style
properties AutoAnimate writes (an own accessor on each item's style
object that records, then calls CSSOM's `setProperty`), and each
`insertBefore`, `appendChild` and `remove` it makes. The case's own calls
are not recorded. The result is tests/auto_animate/auto-animate-0.10.2-oracle.json,
which `cargo test --test auto_animate_values` checks.

`pixels` runs the `pixels` cases on the main thread and captures the listed
frames after two of Chromium's own frames. It compares them with Div's from
`cargo test --test auto_animate_pixels -- --ignored`
(out/auto_animate/div/<case>-<frame>.png).
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from react_spring_oracle import ROOT, Server, load_harness  # noqa: E402

CASES = ROOT / "tests" / "auto_animate" / "cases.json"
ORACLE = ROOT / "tests" / "auto_animate" / "auto-animate-0.10.2-oracle.json"
MODULE = "/fidelity/web/auto-animate-0.10.2/index.mjs"
OUT = ROOT / "out" / "auto_animate"

PRELUDE = """
const __nativeSetTimeout = window.setTimeout.bind(window);
window.__nativeRaf = window.requestAnimationFrame.bind(window);
const __turn = () => new Promise(resolve => __nativeSetTimeout(resolve, 0));
window.__now = 0;
window.__log = [];
window.__scenario = false;
performance.now = () => __now;
Date.now = () => Math.floor(__now);
Math.random = () => 0.5;

// Timers on the virtual clock, ordered by due time, then by id.
let __timerId = 0;
const __timers = new Map();
const __delay = delay => Math.max(0, Number(delay) || 0);
window.setTimeout = (fn, delay, ...args) => {
  const id = ++__timerId;
  __timers.set(id, { due: __now + __delay(delay), fn, args, interval: null });
  return id;
};
window.setInterval = (fn, delay, ...args) => {
  const id = ++__timerId;
  __timers.set(id, { due: __now + __delay(delay), fn, args, interval: __delay(delay) });
  return id;
};
window.clearTimeout = window.clearInterval = id => { __timers.delete(id); };
const __runTimers = async () => {
  for (;;) {
    let next = null;
    for (const [id, timer] of __timers) {
      if (timer.due <= __now && (next === null || timer.due < next[1].due)) next = [id, timer];
    }
    if (next === null) return;
    const [id, timer] = next;
    if (timer.interval === null) __timers.delete(id);
    else timer.due += timer.interval;
    timer.fn(...timer.args);
    await __turn();
  }
};

let __rafQueue = [];
window.requestAnimationFrame = callback => { __rafQueue.push(callback); return __rafQueue.length; };
window.cancelAnimationFrame = () => {};
let __idleQueue = [];
window.requestIdleCallback = callback => { __idleQueue.push(callback); return __idleQueue.length; };
window.cancelIdleCallback = () => {};

class __Inert { observe() {} unobserve() {} disconnect() {} takeRecords() { return []; } }
window.ResizeObserver = __Inert;
window.IntersectionObserver = __Inert;

const __key = node => (node && node.dataset && node.dataset.k) ?? (node ? node.nodeName : null);

// Animations: recorded, paused, and seeked by the pump.
const __nativeAnimate = Element.prototype.animate;
const __nativeFinish = Animation.prototype.finish;
const __nativeCancel = Animation.prototype.cancel;
const __anims = [];
Element.prototype.animate = function (keyframes, options) {
  const animation = __nativeAnimate.call(this, keyframes, options);
  animation.pause();
  animation.currentTime = 0;
  const record = { animation, start: __now, duration: options.duration, done: false, fired: false };
  animation.__id = __anims.length;
  __anims.push(record);
  animation.addEventListener("finish", () => { record.fired = true; });
  __log.push(["animate", __key(this), JSON.parse(JSON.stringify(keyframes)), JSON.parse(JSON.stringify(options))]);
  return animation;
};
Animation.prototype.cancel = function () {
  if (this.__id !== undefined) { __log.push(["cancel", this.__id]); __anims[this.__id].done = true; }
  return __nativeCancel.call(this);
};
let __finishCalled = false;
Animation.prototype.finish = function () {
  if (this.__id !== undefined) { __log.push(["finish", this.__id]); __anims[this.__id].done = true; }
  __finishCalled = true;
  return __nativeFinish.call(this);
};

// AutoAnimate's own DOM operations (the case's are made with __scenario set).
const __nativeInsertBefore = Node.prototype.insertBefore;
const __nativeAppendChild = Node.prototype.appendChild;
const __nativeRemove = Element.prototype.remove;
Node.prototype.insertBefore = function (node, child) {
  if (!__scenario) __log.push(["insertBefore", __key(this), __key(node), __key(child)]);
  return __nativeInsertBefore.call(this, node, child);
};
Node.prototype.appendChild = function (node) {
  if (!__scenario) __log.push(["appendChild", __key(this), __key(node)]);
  return __nativeAppendChild.call(this, node);
};
Element.prototype.remove = function () {
  if (!__scenario) __log.push(["remove", __key(this)]);
  return __nativeRemove.call(this);
};
// A finish event is dispatched at Chromium's next rendering. Wait for two
// of Chromium's frames after a frame step that finished an animation, so
// that the event lands in the same virtual frame.
window.__flushFinish = async () => {
  if (!__finishCalled) return;
  __finishCalled = false;
  await new Promise(resolve => __nativeRaf(() => __nativeRaf(resolve)));
};
window.__scenarioDo = fn => { __scenario = true; try { fn(); } finally { __scenario = false; } };

// The style properties AutoAnimate assigns. Chromium resolves
// el.style.<camelCase> through a named property interceptor, so the
// recorder is an own accessor on each item's style object.
const __props = {
  position: "position", top: "top", left: "left", width: "width", height: "height",
  margin: "margin", pointerEvents: "pointer-events", transformOrigin: "transform-origin",
  zIndex: "z-index",
};
window.__record = el => {
  const style = el.style;
  for (const [prop, css] of Object.entries(__props)) {
    Object.defineProperty(style, prop, {
      configurable: true,
      get() { return CSSStyleDeclaration.prototype.getPropertyValue.call(this, css); },
      set(value) {
        __log.push(["style", __key(el), css, String(value)]);
        CSSStyleDeclaration.prototype.setProperty.call(this, css, value);
      },
    });
  }
};

window.__pump = async now => {
  __now = now;
  await __runTimers();
  const ended = [];
  for (const record of __anims) {
    if (record.done) continue;
    // In seconds, as Chromium's default AnimationTimeDelta computes an
    // animation's current time from timeline times.
    const time = now / 1000 - record.start / 1000;
    if (time >= record.duration / 1000) {
      record.done = true;
      ended.push(record);
      __log.push(["end", record.animation.__id]);
      __nativeFinish.call(record.animation);
    } else {
      record.animation.currentTime = time * 1000;
    }
  }
  await __turn();
  if (ended.length) {
    await new Promise(resolve => __nativeRaf(() => __nativeRaf(resolve)));
    for (const record of ended) {
      if (!record.fired) throw new Error("no finish event for animation " + record.animation.__id);
    }
  }
  const raf = __rafQueue;
  __rafQueue = [];
  for (const callback of raf) { callback(now); await __turn(); }
  const idle = __idleQueue;
  __idleQueue = [];
  for (const callback of idle) { callback({ didTimeout: false, timeRemaining: () => 0 }); await __turn(); }
};
"""

RUNNER = f"""
import {{ autoAnimate }} from "{MODULE}";
window.setupCase = (c, colors) => {{
  const time = k => k * 1000 / 60;
  const parent = document.getElementById("p");
  const items = new Map();
  const item = k => {{
    if (!items.has(k)) {{
      const el = document.createElement("div");
      el.dataset.k = String(k);
      el.className = "i c" + (k % colors);
      __record(el);
      items.set(k, el);
    }}
    return items.get(k);
  }};
  __record(parent);
  __scenarioDo(() => {{ for (let k = 0; k < c.items; k++) parent.appendChild(item(k)); }});
  let controller = null;
  const act = async k => {{
    __now = time(k);
    for (const call of c.calls[String(k)] ?? []) {{
      const [kind, ...args] = call;
      if (kind === "auto") controller = autoAnimate(parent, args[0] ?? {{}});
      else if (kind === "append") __scenarioDo(() => parent.appendChild(item(args[0])));
      else if (kind === "insert") __scenarioDo(() => parent.insertBefore(item(args[0]), item(args[1])));
      else if (kind === "remove") __scenarioDo(() => item(args[0]).remove());
      else if (kind === "disable") controller.disable();
      else if (kind === "enable") controller.enable();
      else if (kind === "destroy") controller.destroy();
      else throw new Error("unknown call " + kind);
    }}
    await __turn();
    await __flushFinish();
  }};
  window.__case = {{
    frames: c.frames,
    act,
    async frame(k) {{ await __pump(time(k)); await act(k); }},
    take() {{ const log = __log; __log = []; return log; }},
  }};
}};
window.__ready = true;
"""


def stylesheet(spec: dict, page: str) -> str:
    rules = spec["pages"][page]
    colors = "".join(
        f".c{i} {{ background: {color}; }}\n" for i, color in enumerate(spec["colors"])
    )
    return (
        "html, body { margin: 0; background: white; overflow: hidden; }\n"
        f"#p {{ {rules['parent']}; }}\n"
        f".i {{ {rules['item']}; }}\n"
        f"{colors}"
    )


def page_html(spec: dict, page: str) -> str:
    return (
        "<!doctype html>\n"
        f"<style>{stylesheet(spec, page)}</style>\n"
        f"<script>{PRELUDE}</script>\n"
        f'<script type="module">{RUNNER}</script>\n'
        '<div id="p" data-k="p"></div>\n'
    )


def write_pages(spec: dict) -> dict:
    OUT.mkdir(parents=True, exist_ok=True)
    pages = {}
    for page in spec["pages"]:
        path = OUT / f"oracle-{page}.html"
        path.write_text(page_html(spec, page))
        pages[page] = path
    return pages


def open_case(browser, server: Server, pages: dict, spec: dict, case: dict) -> None:
    reduced = case.get("reducedMotion", False)
    browser.page.command(
        "Emulation.setEmulatedMedia",
        {"features": [{"name": "prefers-reduced-motion", "value": "reduce" if reduced else ""}]},
    )
    browser.page.command("Page.navigate", {"url": server.url(pages[case["page"]])})
    browser.wait_loaded()
    for _ in range(200):
        if browser.evaluate("window.__ready === true"):
            break
        browser.evaluate("new Promise(r => setTimeout(r, 10))", await_promise=True)
    else:
        raise TimeoutError("AutoAnimate did not load")
    browser.evaluate(
        f"setupCase({json.dumps(case)}, {len(spec['colors'])}); __log = []; __case.act(0)",
        await_promise=True,
    )


def values(div_checkout: Path, chrome_dir: Path) -> int:
    harness, capture = load_harness(div_checkout)
    spec = json.loads(CASES.read_text())
    pages = write_pages(spec)
    server = Server()
    browser = capture.Browser(harness, chrome_dir, [], False)
    oracle = {
        "autoAnimate": "0.10.2",
        "chromium": "147.0.7727.50",
        "comment": "Per frame, what AutoAnimate did during frame 0's calls and during each "
        "later frame: animate [key, keyframes, options], cancel/finish/end [animation], "
        "style [key, property, value], insertBefore [parent, node, child], appendChild "
        "[parent, node], remove [node]. Recorded by tools/auto_animate_oracle.py values.",
        "cases": {},
    }
    try:
        for case in spec["cases"]:
            open_case(browser, server, pages, spec, case)
            result = browser.evaluate(
                """(async () => {
                  const frames = [__case.take()];
                  for (let k = 1; k <= __case.frames; k++) {
                    await __case.frame(k);
                    frames.push(__case.take());
                  }
                  return JSON.stringify(frames);
                })()""",
                await_promise=True,
            )
            oracle["cases"][case["name"]] = {"frames": json.loads(result)}
    finally:
        browser.close()
        server.close()
    head = {key: value for key, value in oracle.items() if key != "cases"}
    out = [json.dumps(head, indent=1)[:-2] + ',\n "cases": {']
    names = list(oracle["cases"])
    for n, name in enumerate(names):
        frames = oracle["cases"][name]["frames"]
        lines = [f"   {json.dumps(f, separators=(',', ':'))}" for f in frames]
        out.append(f'  {json.dumps(name)}: {{"frames": [\n' + ",\n".join(lines) + "\n  ]}"
                   + ("," if n + 1 < len(names) else ""))
    out.append(" }\n}\n")
    ORACLE.write_text("\n".join(out))
    frames = sum(len(c["frames"]) for c in oracle["cases"].values())
    events = sum(len(f) for c in oracle["cases"].values() for f in c["frames"])
    print(f"{len(spec['cases'])} cases, {frames} frames, {events} events -> "
          f"{ORACLE.relative_to(ROOT)}")
    return 0


# Captured on the main thread, as tools/motion_oracle.py does.
FRAME_FLAGS = ["--disable-threaded-animation"]


def pixels(div_checkout: Path, chrome_dir: Path) -> int:
    from PIL import Image, ImageChops

    harness, capture = load_harness(div_checkout)
    spec = json.loads(CASES.read_text())
    by_name = {case["name"]: case for case in spec["cases"]}
    frames = spec["pixels"]["frames"]
    (OUT / "chromium").mkdir(parents=True, exist_ok=True)
    pages = write_pages(spec)
    server = Server()
    rows = []
    try:
        for name in spec["pixels"]["cases"]:
            browser = capture.Browser(harness, chrome_dir, FRAME_FLAGS, False)
            try:
                open_case(browser, server, pages, spec, by_name[name])
                browser.screenshot()  # warm-up: a new surface's first read can be stale
                for k in range(0, max(frames) + 1):
                    if k > 0:
                        browser.evaluate(f"__case.frame({k})", await_promise=True)
                    if k not in frames:
                        continue
                    browser.evaluate(
                        "new Promise(r => __nativeRaf(() => __nativeRaf(r)))",
                        await_promise=True,
                    )
                    png = browser.screenshot()
                    chromium_png = OUT / "chromium" / f"{name}-{k}.png"
                    chromium_png.write_bytes(png)
                    div_png = OUT / "div" / f"{name}-{k}.png"
                    if not div_png.exists():
                        rows.append((name, k, "no Div frame", "", ""))
                        continue
                    a = Image.open(chromium_png).convert("RGBA")
                    b = Image.open(div_png).convert("RGBA")
                    if a.size != b.size:
                        rows.append((name, k, f"size {a.size} vs {b.size}", "", ""))
                        continue
                    diff = ImageChops.difference(a, b)
                    differing = sum(1 for p in diff.getdata() if any(p))
                    worst = max(band_max for _, band_max in diff.getextrema())
                    rows.append((name, k, "exact" if differing == 0 else "differs",
                                 differing, worst))
            finally:
                browser.close()
    finally:
        server.close()
    print("| case | frame | result | differing pixels | max channel delta |")
    print("| --- | ---: | --- | ---: | ---: |")
    for row in rows:
        print("| " + " | ".join(str(v) for v in row) + " |")
    exact = sum(1 for row in rows if row[2] == "exact")
    print(f"\n{exact} of {len(rows)} frames exact")
    return 0


def main() -> int:
    command, div_checkout, chrome_dir = sys.argv[1], Path(sys.argv[2]), Path(sys.argv[3])
    if command == "values":
        return values(div_checkout.resolve(), chrome_dir.resolve())
    if command == "pixels":
        return pixels(div_checkout.resolve(), chrome_dir.resolve())
    raise SystemExit(f"unknown command {command}")


if __name__ == "__main__":
    sys.exit(main())
