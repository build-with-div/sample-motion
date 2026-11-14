#!/usr/bin/env python3
"""Run react-spring 10.1.2 in pinned Chromium: the oracle for the Rust port.

Run in the Linux/WSL environment that holds Div's pinned Chromium:

    python3 tools/react_spring_oracle.py values DIV_CHECKOUT CHROME_DIR
    python3 tools/react_spring_oracle.py pixels DIV_CHECKOUT CHROME_DIR

The page loads the vendored, unmodified ES modules of
fidelity/web/react-spring-10.1.2 through an import map, over a local HTTP
server (Chromium does not load modules from file:// pages). `react` maps to
a stub: the core imports React, but `SpringValue` only touches
`React.createContext` at load time.

Time is the rAF pump that docs/q13-chromium-capture.md measured as
byte-stable: `requestAnimationFrame` and `performance.now` are replaced
before react-spring loads, and frame k runs the queued callbacks at
k * 1000 / fps ms. Each callback is followed by a macrotask turn, so
promise reactions run between callbacks as they do in a browser frame.

`values` runs every case of tests/react_spring/cases.json in a fresh page
(react-spring's frame loop is module state) and writes `spring.get()`,
`spring.velocity` and `spring.isAnimating` after each frame to
tests/react_spring/react-spring-10.1.2-oracle.json, which
`cargo test --test react_spring_values` checks.

`pixels` runs the `pixels` cases on the main thread, writes each spring's
value to the box's inline style after every frame (what `animated` does on
change), and captures the listed frames after two of Chromium's own frames
(its rAF, saved before the pump replaced it). It compares them with Div's
from
`cargo test --test react_spring_pixels -- --ignored`
(out/react-spring/div/<case>-<frame>.png).
"""

from __future__ import annotations

import functools
import http.server
import json
import sys
import threading
from pathlib import Path
from urllib.parse import quote

ROOT = Path(__file__).resolve().parents[1]
CASES = ROOT / "tests" / "react_spring" / "cases.json"
ORACLE = ROOT / "tests" / "react_spring" / "react-spring-10.1.2-oracle.json"
VENDOR = "fidelity/web/react-spring-10.1.2"
OUT = ROOT / "out" / "react-spring"

# The same pump as docs/q13-chromium-capture.md, with a macrotask turn after
# each callback so that microtasks (react-spring's chained updates) run
# between callbacks, as HTML's "run the animation frame callbacks" does.
RAF_PUMP = """
window.__nativeRaf = window.requestAnimationFrame.bind(window);
window.__queue = [];
window.__now = 0;
window.requestAnimationFrame = callback => { __queue.push(callback); return __queue.length; };
window.cancelAnimationFrame = () => {};
performance.now = () => __now;
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
"""

REACT_STUB = (
    "export const createContext = value => ({ Provider: null, Consumer: null, _currentValue: value });"
    "export const createElement = () => null; export const forwardRef = render => render;"
    "const hook = () => { throw new Error('React hooks are not available in the oracle'); };"
    "export const useContext = hook, useMemo = hook, useRef = hook, useState = hook,"
    " useEffect = hook, useLayoutEffect = hook, useCallback = hook;"
)

IMPORT_MAP = {
    "imports": {
        "react": "data:text/javascript," + quote(REACT_STUB),
        "@react-spring/rafz": f"/{VENDOR}/rafz/react-spring_rafz.modern.mjs",
        "@react-spring/shared": f"/{VENDOR}/shared/react-spring_shared.modern.mjs",
        "@react-spring/animated": f"/{VENDOR}/animated/react-spring_animated.modern.mjs",
        "@react-spring/types": f"/{VENDOR}/types/react-spring_types.modern.mjs",
        "@react-spring/core": f"/{VENDOR}/core/react-spring_core.modern.mjs",
    }
}

RUNNER = """
import * as RS from "@react-spring/core";
const reviveConfig = value => {
  if (typeof value === "string") return RS.config[value];
  const config = { ...value };
  if (typeof config.easing === "string") config.easing = RS.easings[config.easing];
  return config;
};
const reviveProps = value => {
  const props = {};
  for (const [key, v] of Object.entries(value)) {
    if (key === "target") continue;
    if (key === "to" && Array.isArray(v)) props.to = v.map(reviveProps);
    else if (key === "config") props.config = reviveConfig(v);
    else props[key] = v;
  }
  return props;
};
window.setupCase = c => {
  const time = k => k * 1000 / (c.fps ?? 60);
  const box = document.getElementById("b");
  const springs = c.springs.map(spec => new RS.SpringValue(reviveProps(spec)));
  const act = async k => {
    __now = time(k);
    for (const action of c.actions) {
      if (action.frame !== k) continue;
      const spring = springs[action.spring];
      if (action.start) spring.start(reviveProps(action.start));
      else if (action.stop) spring.stop();
    }
    await __turn();
  };
  window.__case = {
    frames: c.frames,
    act,
    async frame(k) { await __pump(time(k)); await act(k); },
    sample: () => springs.map(s => [s.get(), s.velocity, s.isAnimating]),
    write() {
      springs.forEach((s, i) => {
        const v = s.get();
        if (c.springs[i].target === "x") box.style.transform = `translate(${v}px, 0px)`;
        else box.style.opacity = String(v);
      });
    },
  };
};
window.__ready = true;
"""

STYLE = """
html, body { margin: 0; background: white; overflow: hidden; }
#b { position: absolute; left: 20px; top: 20px; width: 60px; height: 60px;
     background: blue; }
"""


def page_html() -> str:
    return (
        "<!doctype html>\n"
        f"<style>{STYLE}</style>\n"
        f"<script>{RAF_PUMP}</script>\n"
        f'<script type="importmap">{json.dumps(IMPORT_MAP)}</script>\n'
        f'<script type="module">{RUNNER}</script>\n'
        '<div id="b"></div>\n'
    )


class Server:
    """Serve the repository root on 127.0.0.1."""

    def __init__(self):
        handler = functools.partial(QuietHandler, directory=str(ROOT))
        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)
        self.thread.start()

    def url(self, path: Path) -> str:
        return f"http://127.0.0.1:{self.httpd.server_port}/{path.relative_to(ROOT).as_posix()}"

    def close(self) -> None:
        self.httpd.shutdown()


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, format, *args):  # noqa: A002
        pass


def load_harness(div_checkout: Path):
    sys.path.insert(0, str(div_checkout / "tools" / "accountability"))
    sys.path.insert(0, str(ROOT / "tools"))
    import run_all_pixel_comparisons as harness  # noqa: E402
    import chromium_capture_stability as capture  # noqa: E402

    capture.EventClient = capture.make_event_client(harness)
    return harness, capture


def open_case(browser, server: Server, page: Path, case: dict) -> None:
    browser.page.command("Page.navigate", {"url": server.url(page)})
    browser.wait_loaded()
    for _ in range(200):
        if browser.evaluate("window.__ready === true"):
            break
        browser.evaluate("new Promise(r => setTimeout(r, 10))", await_promise=True)
    else:
        raise TimeoutError("react-spring did not load")
    browser.evaluate(f"setupCase({json.dumps(case)}); __case.act(0)", await_promise=True)


def values(div_checkout: Path, chrome_dir: Path) -> int:
    harness, capture = load_harness(div_checkout)
    cases = json.loads(CASES.read_text())["cases"]
    OUT.mkdir(parents=True, exist_ok=True)
    page = OUT / "oracle.html"
    page.write_text(page_html())
    server = Server()
    browser = capture.Browser(harness, chrome_dir, [], False)
    oracle = {
        "react-spring": "10.1.2",
        "chromium": "147.0.7727.50",
        "comment": "[spring.get(), spring.velocity, spring.isAnimating] for each spring "
        "after each frame's callbacks and actions, frames 0..frames, recorded by "
        "tools/react_spring_oracle.py values.",
        "cases": {},
    }
    try:
        for case in cases:
            open_case(browser, server, page, case)
            result = browser.evaluate(
                """(async () => {
                  const frames = [__case.sample()];
                  for (let k = 1; k <= __case.frames; k++) {
                    await __case.frame(k);
                    frames.push(__case.sample());
                  }
                  return JSON.stringify(frames);
                })()""",
                await_promise=True,
            )
            oracle["cases"][case["name"]] = {"frames": json.loads(result)}
    finally:
        browser.close()
        server.close()
    # One line per case keeps the file reviewable without one line per number.
    head = {key: value for key, value in oracle.items() if key != "cases"}
    lines = [
        f"  {json.dumps(name)}: {json.dumps(value, separators=(',', ':'))}"
        for name, value in oracle["cases"].items()
    ]
    ORACLE.write_text(
        json.dumps(head, indent=1)[:-2] + ',\n "cases": {\n' + ",\n".join(lines) + "\n }\n}\n"
    )
    frames = sum(len(c["frames"]) for c in oracle["cases"].values())
    print(f"{len(cases)} cases, {frames} frames -> {ORACLE.relative_to(ROOT)}")
    return 0


# Captured on the main thread, as tools/motion_oracle.py does.
FRAME_FLAGS = ["--disable-threaded-animation"]


def pixels(div_checkout: Path, chrome_dir: Path) -> int:
    from PIL import Image, ImageChops

    harness, capture = load_harness(div_checkout)
    spec = json.loads(CASES.read_text())
    by_name = {case["name"]: case for case in spec["cases"]}
    frames = spec["pixels"]["frames"]
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "chromium").mkdir(parents=True, exist_ok=True)
    page = OUT / "oracle.html"
    page.write_text(page_html())
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
                    browser.evaluate("__case.write()")
                    if k not in frames:
                        continue
                    # Let the style write reach two of Chromium's own frames
                    # (its rAF, saved before the pump replaced it), as
                    # tools/motion_oracle.py waits after a seek.
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
