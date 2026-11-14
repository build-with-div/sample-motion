#!/usr/bin/env python3
"""Run Motion 14.14.0 in pinned Chromium: the oracle for the Rust port.

Run in the Linux/WSL environment that holds Div's pinned Chromium:

    python3 tools/motion_oracle.py values DIV_CHECKOUT CHROME_DIR
    python3 tools/motion_oracle.py pixels DIV_CHECKOUT CHROME_DIR

`values` runs every case of tests/motion/cases.json through Motion's
`animate()` with `Element.prototype.animate` wrapped, and writes the exact
arguments Motion passed to it to tests/motion/motion-14.14.0-oracle.json.
`cargo test --test motion_native` checks the port against that file.

`pixels` renders the `pixels` cases: Motion starts the animation, then every
animation in `document.getAnimations()` is paused and seeked to each time,
the method docs/q13-chromium-capture.md measured as byte-stable. It compares
with Div's frames from `cargo test --test motion_pixels -- --ignored`
(out/motion/div/<case>-<t>.png) and prints a Markdown table.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CASES = ROOT / "tests" / "motion" / "cases.json"
ORACLE = ROOT / "tests" / "motion" / "motion-14.14.0-oracle.json"
MOTION = ROOT / "fidelity" / "web" / "motion-14.14.0" / "motion.js"
OUT = ROOT / "out" / "motion"

RECORDER = """
window.__calls = [];
const originalAnimate = Element.prototype.animate;
const finite = (key, value) =>
  typeof value === "number" && !Number.isFinite(value) ? String(value) : value;
Element.prototype.animate = function (keyframes, options) {
  if (this.dataset && this.dataset.record) {
    window.__calls.push(JSON.parse(JSON.stringify({ keyframes, options }, finite)));
  }
  return originalAnimate.call(this, keyframes, options);
};
"""

STYLE = """
html, body { margin: 0; background: white; overflow: hidden; }
#b { position: absolute; left: 20px; top: 20px; width: 60px; height: 60px;
     background-color: blue; }
"""


def transform_string(frame: dict) -> str:
    return f"translate({frame['tx']}px, {frame['ty']}px) rotate({frame['rotate']}deg)"


def js_case(case: dict) -> dict:
    """The case as Motion receives it."""
    keyframes = case["keyframes"]
    if case["value"] == "transform":
        keyframes = [transform_string(frame) for frame in keyframes]
    return {"value": case["value"], "keyframes": keyframes, "transition": case["transition"]}


# Restores "Infinity" inside the transition before calling Motion.
REVIVE = """
const revive = value => value === "Infinity" ? Infinity
  : Array.isArray(value) ? value.map(revive)
  : value && typeof value === "object"
    ? Object.fromEntries(Object.entries(value).map(([k, v]) => [k, revive(v)]))
  : value;
"""


def page(pages: Path, name: str, body: str) -> Path:
    path = pages / f"{name}.html"
    path.write_text(
        "<!doctype html>\n"
        f"<style>{STYLE}</style>\n"
        f"<script>{RECORDER}{REVIVE}</script>\n"
        f"<script src=\"file://{MOTION}\"></script>\n"
        f"{body}\n"
    )
    return path


def load_harness(div_checkout: Path):
    sys.path.insert(0, str(div_checkout / "tools" / "accountability"))
    sys.path.insert(0, str(ROOT / "tools"))
    import run_all_pixel_comparisons as harness  # noqa: E402
    import chromium_capture_stability as capture  # noqa: E402

    capture.EventClient = capture.make_event_client(harness)
    return harness, capture


def values(div_checkout: Path, chrome_dir: Path) -> int:
    harness, capture = load_harness(div_checkout)
    cases = json.loads(CASES.read_text())["values"]
    pages = OUT / "html"
    pages.mkdir(parents=True, exist_ok=True)
    browser = capture.Browser(harness, chrome_dir, [], False)
    oracle = {
        "motion": "14.14.0",
        "chromium": "147.0.7727.50",
        "comment": "Arguments Motion passed to element.animate() per case, "
        "recorded by tools/motion_oracle.py values. Non-finite numbers are strings.",
        "cases": {},
    }
    try:
        browser.navigate(page(pages, "values", "<body></body>"))
        browser.wait_loaded()
        for case in cases:
            spec = json.dumps(js_case(case))
            result = browser.evaluate(
                f"""(async spec => {{
                  const el = document.createElement('div');
                  el.dataset.record = '1';
                  document.body.appendChild(el);
                  window.__calls = [];
                  let error = null;
                  try {{
                    Motion.animate(el, {{ [spec.value]: spec.keyframes }}, revive(spec.transition));
                  }} catch (e) {{ error = String(e); }}
                  await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));
                  return JSON.stringify({{ calls: window.__calls, error }});
                }})({spec})""",
                await_promise=True,
            )
            oracle["cases"][case["name"]] = json.loads(result)
    finally:
        browser.close()
    ORACLE.write_text(json.dumps(oracle, indent=1) + "\n")
    calls = sum(len(c["calls"]) for c in oracle["cases"].values())
    print(f"{len(cases)} cases, {calls} element.animate() calls -> {ORACLE.relative_to(ROOT)}")
    return 0


def pixels(div_checkout: Path, chrome_dir: Path) -> int:
    from PIL import Image, ImageChops

    harness, capture = load_harness(div_checkout)
    spec = json.loads(CASES.read_text())
    by_name = {case["name"]: case for case in spec["values"]}
    times = spec["pixels"]["times"]
    pages = OUT / "html"
    pages.mkdir(parents=True, exist_ok=True)
    (OUT / "chromium").mkdir(parents=True, exist_ok=True)
    rows = []
    for name in spec["pixels"]["cases"]:
        case = js_case(by_name[name])
        body = (
            '<div id="b"></div>\n<script>\n'
            f"const spec = {json.dumps(case)};\n"
            "Motion.animate(document.getElementById('b'), "
            "{ [spec.value]: spec.keyframes }, revive(spec.transition));\n"
            "</script>"
        )
        browser = capture.Browser(harness, chrome_dir, [], False)
        try:
            browser.navigate(page(pages, f"pixels-{name}", body))
            browser.wait_loaded()
            # Motion resolves keyframes on its next frame, then starts WAAPI.
            browser.evaluate(
                "new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))",
                await_promise=True,
            )
            count = browser.evaluate("document.getAnimations().length")
            browser.screenshot()  # warm-up: a new surface's first read can be stale
            for t in times:
                browser.evaluate(
                    "document.getAnimations().forEach(a => { a.pause(); "
                    f"a.currentTime = {t}; }})"
                )
                png = browser.screenshot()
                chromium_png = OUT / "chromium" / f"{name}-{t}.png"
                chromium_png.write_bytes(png)
                div_png = OUT / "div" / f"{name}-{t}.png"
                if not div_png.exists():
                    rows.append((name, t, count, "no Div frame", "", ""))
                    continue
                a = Image.open(chromium_png).convert("RGBA")
                b = Image.open(div_png).convert("RGBA")
                if a.size != b.size:
                    rows.append((name, t, count, f"size {a.size} vs {b.size}", "", ""))
                    continue
                diff = list(ImageChops.difference(a, b).getdata())
                differing = sum(1 for p in diff if any(p))
                worst = max(max(p) for p in diff)
                rows.append((name, t, count, "exact" if differing == 0 else "differs",
                             differing, worst))
        finally:
            browser.close()
    print("| case | t (ms) | Chromium animations | result | differing pixels | max channel delta |")
    print("| --- | ---: | ---: | --- | ---: | ---: |")
    for row in rows:
        print("| " + " | ".join(str(v) for v in row) + " |")
    exact = sum(1 for row in rows if row[3] == "exact")
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
