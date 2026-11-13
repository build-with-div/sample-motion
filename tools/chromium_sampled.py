#!/usr/bin/env python3
"""Render the sampled-time animation cases in pinned Chromium and compare.

Run in the Linux/WSL environment that holds Div's pinned Chromium, after
`cargo test --test fidelity_sampled -- --ignored` wrote Div's side:

    python3 tools/chromium_sampled.py DIV_CHECKOUT CHROME_DIR

DIV_CHECKOUT is a build-with-div/div checkout; its harness
(tools/accountability/run_all_pixel_comparisons.py, render_chrome) does the
capture, so device metrics, fonts and capture rules equal Div's census.
CHROME_DIR holds the `chrome` binary (Chromium 147.0.7727.50).

Chromium samples time t with CSS only: `animation-delay: -t; paused`.
Keep CASES equal to tests/fidelity_sampled.rs.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TIMES = [0, 100, 250, 500, 750, 1000]
CASES = {
    "opacity": ("linear", "opacity: 0", "opacity: 1"),
    "translate-rotate-ease": (
        "cubic-bezier(0.25, 0.1, 0.25, 1)",
        "transform: translate(0px, 0px) rotate(0deg)",
        "transform: translate(100px, 0px) rotate(45deg)",
    ),
    "background-color": (
        "linear",
        "background-color: rgb(255, 0, 0)",
        "background-color: rgb(0, 0, 255)",
    ),
    "width-steps": ("steps(4, jump-end)", "width: 20px", "width: 120px"),
    "scale-linear-stops": (
        "linear(0, 0.25 75%, 1)",
        "transform: scale(1)",
        "transform: scale(2)",
    ),
    "scale-linear-stops-percent": (
        "linear(0, 0.25 75%, 1)",
        "transform: scale(1)",
        "transform: scale(2)",
    ),
    "scale-linear": ("linear", "transform: scale(1)", "transform: scale(2)"),
    # Not animated: separates fractional-scale raster from animation.
    "scale-static-1.75": (None, "transform: scale(1.75)", ""),
    "visibility": ("linear", "visibility: hidden", "visibility: visible"),
}


def html(easing: str | None, start: str, end: str, time_ms: int) -> str:
    if easing is None:
        return f"""<!doctype html>
<style>
html, body {{ margin: 0; background: white; overflow: hidden; }}
#b {{ position: absolute; left: 20px; top: 20px; width: 60px; height: 60px;
      background-color: blue; {start}; }}
</style>
<div id="b"></div>
"""
    return f"""<!doctype html>
<style>
html, body {{ margin: 0; background: white; overflow: hidden; }}
#b {{ position: absolute; left: 20px; top: 20px; width: 60px; height: 60px;
      background-color: blue;
      animation: k 1000ms {easing} -{time_ms}ms 1 normal both paused; }}
@keyframes k {{ from {{ {start}; }} to {{ {end}; }} }}
</style>
<div id="b"></div>
"""


def main() -> int:
    div_checkout = Path(sys.argv[1]).resolve()
    chrome_dir = Path(sys.argv[2]).resolve()
    sys.path.insert(0, str(div_checkout / "tools" / "accountability"))
    import run_all_pixel_comparisons as harness  # noqa: E402
    from PIL import Image, ImageChops  # noqa: E402

    out = ROOT / "out" / "fidelity"
    (out / "chromium").mkdir(parents=True, exist_ok=True)
    (out / "html").mkdir(parents=True, exist_ok=True)
    rows = []
    for case, (easing, start, end) in CASES.items():
        for time_ms in TIMES:
            name = f"{case}-{time_ms}"
            page = out / "html" / f"{name}.html"
            page.write_text(html(easing, start, end, time_ms))
            chromium_png = out / "chromium" / f"{name}.png"
            ok = harness.render_chrome(
                str(page), str(chromium_png), str(chrome_dir / "chrome"), str(chrome_dir),
                logical_width=200, logical_height=120, device_scale=1.0,
            )
            if not ok:
                rows.append((case, time_ms, "chromium capture failed", "", ""))
                continue
            div_png = out / "div" / f"{name}.png"
            a = Image.open(chromium_png).convert("RGBA")
            b = Image.open(div_png).convert("RGBA")
            if a.size != b.size:
                rows.append((case, time_ms, f"size {a.size} vs {b.size}", "", ""))
                continue
            diff = ImageChops.difference(a, b)
            data = list(diff.getdata())
            pixels = sum(1 for p in data if any(p))
            worst = max(max(p) for p in data)
            rows.append((case, time_ms, "exact" if pixels == 0 else "differs", pixels, worst))
    print("| case | t (ms) | result | differing pixels | max channel delta |")
    print("| --- | ---: | --- | ---: | ---: |")
    for row in rows:
        print("| " + " | ".join(str(v) for v in row) + " |")
    exact = sum(1 for row in rows if row[2] == "exact")
    print(f"\n{exact} of {len(rows)} samples exact")
    return 0


if __name__ == "__main__":
    sys.exit(main())
