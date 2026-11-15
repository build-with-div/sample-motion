#!/usr/bin/env python3
"""Run anime.js 4.5.0 in pinned Chromium: the oracle for the Rust port.

Run in the Linux/WSL environment that holds Div's pinned Chromium:

    python3 tools/anime_oracle.py values DIV_CHECKOUT CHROME_DIR
    python3 tools/anime_oracle.py pixels DIV_CHECKOUT CHROME_DIR

The page loads the vendored, unmodified ES module bundle
fidelity/web/animejs-4.5.0/anime.esm.js over a local HTTP server (Chromium
does not load modules from file:// pages).

Time is the rAF pump that docs/q13-chromium-capture.md measured as
byte-stable, extended to `Date.now`, which anime.js reads instead of the
frame timestamp: `requestAnimationFrame`, `performance.now` and `Date.now`
are replaced before anime.js loads (its engine captures
`requestAnimationFrame` and starts its clock at load, at time 0). Frame k
runs the queued callbacks with `Date.now()` at floor(k * 1000 / 60) ms,
each followed by a macrotask turn, then the case's calls for frame k.

`values` runs every case of tests/anime/cases.json in a fresh page
(anime.js's engine is module state) and records every assignment to
`el.style.opacity` and `el.style.transform` of the boxes, in order, with
an own accessor on each box's style object that records the value and
calls CSSOM's `setProperty` (the getter is `getPropertyValue`, so reads
return Chromium's serialization). It writes
them per frame to tests/anime/animejs-4.5.0-oracle.json, which
`cargo test --test anime_values` checks.

`pixels` runs the `pixels` cases on the main thread and captures the listed
frames after two of Chromium's own frames (its rAF, saved before the pump
replaced it). It compares them with Div's from
`cargo test --test anime_pixels -- --ignored`
(out/anime/div/<case>-<frame>.png).
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from react_spring_oracle import ROOT, Server, load_harness  # noqa: E402

CASES = ROOT / "tests" / "anime" / "cases.json"
ORACLE = ROOT / "tests" / "anime" / "animejs-4.5.0-oracle.json"
BUNDLE = "/fidelity/web/animejs-4.5.0/anime.esm.js"
OUT = ROOT / "out" / "anime"

# The pump of tools/react_spring_oracle.py plus Date.now, and the setter
# wrappers that record anime.js's style writes.
PRELUDE = """
window.__nativeRaf = window.requestAnimationFrame.bind(window);
window.__queue = [];
window.__now = 0;
window.requestAnimationFrame = callback => { __queue.push(callback); return __queue.length; };
window.cancelAnimationFrame = () => {};
performance.now = () => __now;
Date.now = () => Math.floor(__now);
const __turn = () => new Promise(resolve => setTimeout(resolve, 0));
window.__pump = async now => {
  __now = now;
  const queue = __queue;
  __queue = [];
  for (const callback of queue) {
    callback(now);
    await __turn();
  }
};
window.__styles = [];
window.__writes = [];
// Chromium resolves el.style.<camelCase> through a named property
// interceptor, not prototype accessors, so the recorder is an own accessor
// on each box's style object. Assigning el.style.transform is
// setProperty('transform', value) (CSSOM), and reading it is
// getPropertyValue('transform').
window.__record = style => {
  for (const prop of ["opacity", "transform"]) {
    Object.defineProperty(style, prop, {
      configurable: true,
      get() { return CSSStyleDeclaration.prototype.getPropertyValue.call(this, prop); },
      set(value) {
        __writes.push([__styles.indexOf(this), prop, String(value)]);
        CSSStyleDeclaration.prototype.setProperty.call(this, prop, value);
      },
    });
  }
};
"""

RUNNER = f"""
import * as A from "{BUNDLE}";
const revive = value => {{
  if (Array.isArray(value)) return value.map(revive);
  if (value === null || typeof value !== "object") return value;
  if ("stagger" in value) return A.stagger(revive(value.stagger), revive(value.params ?? {{}}));
  if ("cubicBezier" in value) return A.cubicBezier(...value.cubicBezier);
  if ("steps" in value) return A.steps(...value.steps);
  if ("linear" in value) return A.linear(...value.linear);
  const out = {{}};
  for (const [k, v] of Object.entries(value)) out[k] = revive(v);
  return out;
}};
window.setupCase = c => {{
  const time = k => k * 1000 / 60;
  const els = [...document.querySelectorAll(".b")];
  __styles = els.map(e => e.style);
  __styles.forEach(__record);
  const pick = t => t.map(i => els[i]);
  const timelines = [];
  const act = async k => {{
    __now = time(k);
    for (const call of c.calls[String(k)] ?? []) {{
      const [kind, ...args] = call;
      if (kind === "animate") A.animate(pick(args[0]), revive(args[1]));
      else if (kind === "timeline") timelines.push(A.createTimeline(revive(args[0])));
      else if (kind === "add") timelines[args[0]].add(pick(args[1]), revive(args[2]), revive(args[3]));
      else if (kind === "set") timelines[args[0]].set(pick(args[1]), revive(args[2]), revive(args[3]));
      else if (kind === "label") timelines[args[0]].label(args[1], revive(args[2]));
      else throw new Error("unknown call " + kind);
    }}
    await __turn();
  }};
  window.__case = {{
    frames: c.frames,
    act,
    async frame(k) {{ await __pump(time(k)); await act(k); }},
    take() {{ const w = __writes; __writes = []; return w; }},
  }};
}};
window.__ready = true;
"""

STYLE = """
html, body { margin: 0; background: white; overflow: hidden; }
.b { position: absolute; top: 45px; width: 30px; height: 30px; background: blue; }
"""


def page_html(boxes: int) -> str:
    divs = "".join(
        f'<div class="b" style="left: {10 + 40 * i}px"></div>' for i in range(boxes)
    )
    return (
        "<!doctype html>\n"
        f"<style>{STYLE}</style>\n"
        f"<script>{PRELUDE}</script>\n"
        f'<script type="module">{RUNNER}</script>\n'
        f"{divs}\n"
    )


def open_case(browser, server: Server, page: Path, case: dict) -> None:
    browser.page.command("Page.navigate", {"url": server.url(page)})
    browser.wait_loaded()
    for _ in range(200):
        if browser.evaluate("window.__ready === true"):
            break
        browser.evaluate("new Promise(r => setTimeout(r, 10))", await_promise=True)
    else:
        raise TimeoutError("anime.js did not load")
    browser.evaluate(
        f"__writes = []; setupCase({json.dumps(case)}); __case.act(0)", await_promise=True
    )


def values(div_checkout: Path, chrome_dir: Path) -> int:
    harness, capture = load_harness(div_checkout)
    spec = json.loads(CASES.read_text())
    OUT.mkdir(parents=True, exist_ok=True)
    page = OUT / "oracle.html"
    page.write_text(page_html(spec["boxes"]))
    server = Server()
    browser = capture.Browser(harness, chrome_dir, [], False)
    oracle = {
        "animejs": "4.5.0",
        "chromium": "147.0.7727.50",
        "comment": "Every [box, property, String(value)] assigned to el.style.opacity or "
        "el.style.transform, in order, during frame 0's calls and during each later "
        "frame's callbacks and calls, frames 0..frames, recorded by "
        "tools/anime_oracle.py values.",
        "cases": {},
    }
    try:
        for case in spec["cases"]:
            open_case(browser, server, page, case)
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
    # One line per frame keeps the file reviewable and diffs small.
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
    writes = sum(len(f) for c in oracle["cases"].values() for f in c["frames"])
    print(f"{len(spec['cases'])} cases, {frames} frames, {writes} writes -> "
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
    page = OUT / "oracle.html"
    page.write_text(page_html(spec["boxes"]))
    server = Server()
    rows = []
    try:
        for name in spec["pixels"]["cases"]:
            browser = capture.Browser(harness, chrome_dir, FRAME_FLAGS, False)
            try:
                open_case(browser, server, page, by_name[name])
                browser.screenshot()  # warm-up: a new surface's first read can be stale
                for k in range(0, max(frames) + 1):
                    if k > 0:
                        browser.evaluate(f"__case.frame({k})", await_promise=True)
                    if k not in frames:
                        continue
                    # Let the style writes reach two of Chromium's own frames.
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
