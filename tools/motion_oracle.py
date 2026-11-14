#!/usr/bin/env python3
"""Run Motion 14.14.0 in pinned Chromium: the oracle for the Rust port.

Run in the Linux/WSL environment that holds Div's pinned Chromium:

    python3 tools/motion_oracle.py values DIV_CHECKOUT CHROME_DIR
    python3 tools/motion_oracle.py pixels DIV_CHECKOUT CHROME_DIR
    python3 tools/motion_oracle.py sequences DIV_CHECKOUT CHROME_DIR
    python3 tools/motion_oracle.py sequence-pixels DIV_CHECKOUT CHROME_DIR

`values` runs every case of tests/motion/cases.json through Motion's
`animate()` with `Element.prototype.animate` wrapped, and writes the exact
arguments Motion passed to it to tests/motion/motion-14.14.0-oracle.json.
`cargo test --test motion_native` checks the port against that file.

`pixels` renders the `pixels` cases: Motion starts the animation, then every
animation in `document.getAnimations()` is paused and seeked to each time,
the method docs/q13-chromium-capture.md measured as byte-stable. It compares
with Div's frames from `cargo test --test motion_pixels -- --ignored`
(out/motion/div/<case>-<t>.png) and prints a Markdown table.

`sequences` and `sequence-pixels` do the same for tests/motion/sequences.json:
`stagger()` across several elements and `animate(sequence)`. Calls are
recorded per element index into
tests/motion/motion-14.14.0-sequences-oracle.json, which
`cargo test --test motion_sequence` checks; Div's frames come from
`cargo test --test motion_pixels -- --ignored`.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CASES = ROOT / "tests" / "motion" / "cases.json"
ORACLE = ROOT / "tests" / "motion" / "motion-14.14.0-oracle.json"
SEQUENCES = ROOT / "tests" / "motion" / "sequences.json"
SEQUENCE_ORACLE = ROOT / "tests" / "motion" / "motion-14.14.0-sequences-oracle.json"
MOTION = ROOT / "fidelity" / "web" / "motion-14.14.0" / "motion.js"
OUT = ROOT / "out" / "motion"

RECORDER = """
window.__calls = [];
const originalAnimate = Element.prototype.animate;
const finite = (key, value) =>
  typeof value === "number" && !Number.isFinite(value) ? String(value) : value;
Element.prototype.animate = function (keyframes, options) {
  if (this.dataset && this.dataset.record) {
    const index = this.dataset.index === undefined ? undefined : Number(this.dataset.index);
    window.__calls.push(JSON.parse(JSON.stringify({ index, keyframes, options }, finite)));
  }
  return originalAnimate.call(this, keyframes, options);
};
"""

STYLE = """
html, body { margin: 0; background: white; overflow: hidden; }
#b { position: absolute; left: 20px; top: 20px; width: 60px; height: 60px;
     background-color: blue; }
.b { position: absolute; width: 20px; height: 20px; background-color: blue; }
"""


def box_position(index: int) -> tuple[int, int]:
    """Where element `index` of a sequence case sits (as in tests/motion_pixels.rs)."""
    return 10 + (index % 6) * 30, 10 + (index // 6) * 50


def transform_string(frame: dict) -> str:
    return f"translate({frame['tx']}px, {frame['ty']}px) rotate({frame['rotate']}deg)"


def js_case(case: dict) -> dict:
    """The case as Motion receives it."""
    keyframes = case["keyframes"]
    if case["value"] == "transform":
        keyframes = [transform_string(frame) for frame in keyframes]
    return {"value": case["value"], "keyframes": keyframes, "transition": case["transition"]}


# Restores "Infinity", element subjects ({"__subject": i or [i...]}, from
# window.__els) and stagger() delays ({"stagger": d, ...options}) before
# calling Motion.
REVIVE = """
const subjectOf = s => Array.isArray(s) ? s.map(i => window.__els[i]) : window.__els[s];
const revive = value => value === "Infinity" ? Infinity
  : Array.isArray(value) ? value.map(revive)
  : value && typeof value === "object"
    ? ("__subject" in value ? subjectOf(value.__subject)
      : "stagger" in value
        ? (({ stagger, ...options }) => Motion.stagger(stagger, revive(options)))(value)
      : Object.fromEntries(Object.entries(value).map(([k, v]) => [k, revive(v)])))
  : value;
"""


def js_keyframes(keyframes: dict) -> dict:
    """A keyframes object as Motion receives it: transforms as strings."""
    def convert(name, frame):
        return transform_string(frame) if name == "transform" and isinstance(frame, dict) else frame

    return {
        name: [convert(name, f) for f in frames] if isinstance(frames, list) else convert(name, frames)
        for name, frames in keyframes.items()
    }


def js_item(item):
    """A sequence item as Motion receives it, before `revive`."""
    if isinstance(item, str):
        return item
    if isinstance(item, list):
        subject, keyframes, *transition = item
        return [{"__subject": subject}, js_keyframes(keyframes), *transition]
    if "sequence" in item:
        return {**item, "sequence": [js_item(i) for i in item["sequence"]]}
    return item


def js_sequence_case(case: dict) -> dict:
    """What `RUN_SEQUENCE_CASE` calls: animate(elements, ...) or animate(sequence, ...)."""
    if "sequence" in case:
        return {"sequence": [js_item(i) for i in case["sequence"]], "options": case.get("options", {})}
    return {
        "subject": {"__subject": list(range(case["elements"]))},
        "keyframes": js_keyframes(case["keyframes"]),
        "transition": case["transition"],
    }


RUN_SEQUENCE_CASE = """
const runCase = spec => spec.sequence
  ? Motion.animate(revive(spec.sequence), revive(spec.options))
  : Motion.animate(revive(spec.subject), spec.keyframes, revive(spec.transition));
"""


def page(pages: Path, name: str, body: str) -> Path:
    path = pages / f"{name}.html"
    path.write_text(
        "<!doctype html>\n"
        f"<style>{STYLE}</style>\n"
        f"<script>{RECORDER}{REVIVE}{RUN_SEQUENCE_CASE}</script>\n"
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


def sequences(div_checkout: Path, chrome_dir: Path) -> int:
    harness, capture = load_harness(div_checkout)
    spec = json.loads(SEQUENCES.read_text())
    cases = spec["staggers"] + spec["sequences"]
    pages = OUT / "html"
    pages.mkdir(parents=True, exist_ok=True)
    browser = capture.Browser(harness, chrome_dir, [], False)
    oracle = {
        "motion": "14.14.0",
        "chromium": "147.0.7727.50",
        "comment": "Arguments Motion passed to element.animate() per case, with the "
        "index of the element, recorded by tools/motion_oracle.py sequences. "
        "Non-finite numbers are strings.",
        "cases": {},
    }
    try:
        browser.navigate(page(pages, "sequences", "<body></body>"))
        browser.wait_loaded()
        for case in cases:
            run = json.dumps(js_sequence_case(case))
            result = browser.evaluate(
                f"""(async (spec, count) => {{
                  document.body.replaceChildren();
                  window.__els = Array.from({{ length: count }}, (_, i) => {{
                    const el = document.createElement('div');
                    el.dataset.record = '1';
                    el.dataset.index = String(i);
                    document.body.appendChild(el);
                    return el;
                  }});
                  window.__calls = [];
                  let error = null;
                  try {{ runCase(spec); }} catch (e) {{ error = String(e); }}
                  await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));
                  return JSON.stringify({{ calls: window.__calls, error }});
                }})({run}, {case["elements"]})""",
                await_promise=True,
            )
            oracle["cases"][case["name"]] = json.loads(result)
    finally:
        browser.close()
    SEQUENCE_ORACLE.write_text(json.dumps(oracle, indent=1) + "\n")
    calls = sum(len(c["calls"]) for c in oracle["cases"].values())
    print(f"{len(cases)} cases, {calls} element.animate() calls -> {SEQUENCE_ORACLE.relative_to(ROOT)}")
    return 0


# Frames are captured with every animation on the main thread. With threaded
# (compositor) animation, a paused-and-seeked animation can keep the value
# the compositor held at the real-time moment of the pause, which differs
# from run to run (measured in docs/motion-port.md).
FRAME_FLAGS = ["--disable-threaded-animation"]


def compare_frames(div_checkout: Path, chrome_dir: Path, bodies: list, times: list) -> int:
    """Capture each (name, body) page at `times` with every animation paused
    and seeked, and compare with out/motion/div/<name>-<t>.png."""
    from PIL import Image, ImageChops

    harness, capture = load_harness(div_checkout)
    pages = OUT / "html"
    pages.mkdir(parents=True, exist_ok=True)
    (OUT / "chromium").mkdir(parents=True, exist_ok=True)
    rows = []
    for name, body in bodies:
        browser = capture.Browser(harness, chrome_dir, FRAME_FLAGS, False)
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
                # Let two frames reach the compositor. An animation that was
                # running on the compositor and is seeked into its delay
                # otherwise keeps a stale, run-dependent value in the
                # screenshot (measured in docs/motion-port.md).
                browser.evaluate(
                    "new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))",
                    await_promise=True,
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
                diff = ImageChops.difference(a, b)
                differing = sum(1 for p in diff.getdata() if any(p))
                worst = max(band_max for _, band_max in diff.getextrema())
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


def pixels(div_checkout: Path, chrome_dir: Path) -> int:
    spec = json.loads(CASES.read_text())
    by_name = {case["name"]: case for case in spec["values"]}
    bodies = []
    for name in spec["pixels"]["cases"]:
        case = js_case(by_name[name])
        bodies.append((name, (
            '<div id="b"></div>\n<script>\n'
            f"const spec = {json.dumps(case)};\n"
            "Motion.animate(document.getElementById('b'), "
            "{ [spec.value]: spec.keyframes }, revive(spec.transition));\n"
            "</script>"
        )))
    return compare_frames(div_checkout, chrome_dir, bodies, spec["pixels"]["times"])


def sequence_pixels(div_checkout: Path, chrome_dir: Path) -> int:
    spec = json.loads(SEQUENCES.read_text())
    by_name = {case["name"]: case for case in spec["staggers"] + spec["sequences"]}
    bodies = []
    for name in spec["pixels"]["cases"]:
        case = by_name[name]
        boxes = "".join(
            f'<div class="b" style="left: {x}px; top: {y}px"></div>'
            for x, y in map(box_position, range(case["elements"]))
        )
        bodies.append((name, (
            f"{boxes}\n<script>\n"
            "window.__els = Array.from(document.querySelectorAll('.b'));\n"
            f"runCase({json.dumps(js_sequence_case(case))});\n"
            "</script>"
        )))
    return compare_frames(div_checkout, chrome_dir, bodies, spec["pixels"]["times"])


def main() -> int:
    command, div_checkout, chrome_dir = sys.argv[1], Path(sys.argv[2]), Path(sys.argv[3])
    if command == "values":
        return values(div_checkout.resolve(), chrome_dir.resolve())
    if command == "pixels":
        return pixels(div_checkout.resolve(), chrome_dir.resolve())
    if command == "sequences":
        return sequences(div_checkout.resolve(), chrome_dir.resolve())
    if command == "sequence-pixels":
        return sequence_pixels(div_checkout.resolve(), chrome_dir.resolve())
    raise SystemExit(f"unknown command {command}")


if __name__ == "__main__":
    sys.exit(main())
