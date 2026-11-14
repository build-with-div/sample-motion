#!/usr/bin/env python3
"""Q13: how byte-stable are animation captures in pinned Chromium?

Run in the Linux/WSL environment that holds Div's pinned Chromium:

    python3 tools/chromium_capture_stability.py DIV_CHECKOUT CHROME_DIR [RUNS]

DIV_CHECKOUT is a build-with-div/div checkout. Its harness
(tools/accountability/run_all_pixel_comparisons.py) provides the CDP client
and Chromium's font environment, so captures are made under the same
conditions as Div's census. CHROME_DIR holds the `chrome` binary
(Chromium 147.0.7727.50).

The same animation (translate 0 -> 100 px, rotate 0 -> 45 deg, opacity
0.2 -> 1, 1000 ms, linear) is driven three ways, as the motion packages
drive it:

  css    a CSS @keyframes animation (tailwindcss-animate, Radix)
  waapi  element.animate() (Motion, AutoAnimate)
  raf    a requestAnimationFrame loop writing inline style (GSAP,
         react-spring, Motion's main-thread values)

and sampled at TIMES ms after it starts by three methods:

  paused        CSS: `animation-delay: -t; paused`; WAAPI: pause() and
                currentTime = t. A rAF loop cannot be seeked, so `raf` is
                not applicable.
  virtual-time  Load on real time, then Emulation.setVirtualTimePolicy:
                pause, start the animation, advance a budget to each t
                and capture.
  virtual-time-main
                The same with Chromium's deterministic-mode switches other
                than begin-frame control (no threaded animation or
                scrolling, no checker imaging, all compositor stages before
                draw).
  begin-frame   --deterministic-mode with a target created under
                enableBeginFrameControl; HeadlessExperimental.beginFrame at
                frame times start + t, with the screenshot taken by the
                frame itself.
  raf-pump      For the rAF loop only: requestAnimationFrame and
                performance.now are replaced before the page's scripts, and
                the oracle calls the queued callbacks at 60 Hz frame times
                up to each t (and at t), then captures. Main-thread flags as
                virtual-time-main.

Every run launches a fresh browser. The report gives, per method, scenario
and t: successful captures, distinct decoded-RGBA hashes across runs, the
animation time the page reported at capture (iteration progress x 1000 ms,
or the rAF loop's elapsed time), whether the capture equals the reference
at the nominal t (the `paused` CSS capture of the first run), and whether it
equals a `paused` CSS capture at the time the page itself reported.
Results go to out/q13/results.json and stdout as a Markdown table; the
first run's PNGs go to out/q13/png/.
"""

from __future__ import annotations

import base64
import hashlib
import io
import json
import os
import subprocess
import sys
import tempfile
import time
import traceback
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "out" / "q13"
TIMES = [0, 100, 250, 500, 750, 1000]
WIDTH, HEIGHT = 200, 120
FRAME_MS = 1000.0 / 60.0
WARM_UP = os.environ.get("Q13_NO_WARM_UP") is None
CDP_TIMEOUT = 30
METHODS = ["paused", "virtual-time", "virtual-time-main", "begin-frame", "raf-pump"]

# Installed before the page's scripts by the raf-pump method: the oracle,
# not the browser's frame clock, calls the page's rAF callbacks with
# explicit frame times, as fake-timer test tools do.
RAF_PUMP = """
window.__queue = [];
window.__now = 0;
window.requestAnimationFrame = callback => { __queue.push(callback); return __queue.length; };
window.cancelAnimationFrame = () => {};
performance.now = () => __now;
window.__pump = now => {
  __now = now;
  const queue = __queue;
  __queue = [];
  for (const callback of queue) callback(now);
};"""
# Chromium's deterministic-mode switches other than begin-frame control:
# animations, scrolling and image decode stay on the main thread.
MAIN_THREAD_FLAGS = [
    "--disable-threaded-animation",
    "--disable-threaded-scrolling",
    "--disable-checker-imaging",
    "--run-all-compositor-stages-before-draw",
]
SCENARIOS = ["css", "waapi", "raf"]

FROM_T = "translate(0px, 0px) rotate(0deg)"
TO_T = "translate(100px, 0px) rotate(45deg)"

STYLE = f"""
html, body {{ margin: 0; background: white; overflow: hidden; }}
#b {{ position: absolute; left: 20px; top: 20px; width: 60px; height: 60px;
      background-color: blue; transform: {FROM_T}; opacity: 0.2; }}
@keyframes k {{ from {{ transform: {FROM_T}; opacity: 0.2; }}
               to {{ transform: {TO_T}; opacity: 1; }} }}
#b.run {{ animation: k 1000ms linear 0ms 1 normal both; }}
"""

# window.start() begins the animation; window.probe() reports its time.
SCRIPTS = {
    "css": "window.start = () => b.classList.add('run');",
    "waapi": f"""
window.start = () => {{
  window.anim = b.animate(
    [{{transform: '{FROM_T}', opacity: 0.2}}, {{transform: '{TO_T}', opacity: 1}}],
    {{duration: 1000, easing: 'linear', fill: 'both'}});
}};""",
    "raf": """
window.start = () => {
  let first = null;
  const step = now => {
    if (first === null) first = now;
    const p = Math.min(1, (now - first) / 1000);
    b.style.transform = `translate(${100 * p}px, 0px) rotate(${45 * p}deg)`;
    b.style.opacity = String(0.2 + 0.8 * p);
    window.rafElapsed = now - first;
    if (p < 1) requestAnimationFrame(step);
  };
  requestAnimationFrame(step);
};""",
}

PROBE = """JSON.stringify({
  timeline: document.timeline.currentTime,
  animation: document.getAnimations().length
    ? 1000 * document.getAnimations()[0].effect.getComputedTiming().progress : null,
  raf: window.rafElapsed === undefined ? null : window.rafElapsed,
})"""


def page_html(
    scenario: str, paused_css_at: float | None = None, raf_pump: bool = False
) -> str:
    style = STYLE
    pump = f"<script>{RAF_PUMP}</script>\n" if raf_pump else ""
    if paused_css_at is not None:
        style += (
            f"#b {{ animation: k 1000ms linear -{paused_css_at:.6f}ms 1 normal both paused; }}"
        )
    return (
        f"<!doctype html>\n{pump}<style>{style}</style>\n<div id=\"b\"></div>\n"
        f"<script>const b = document.getElementById('b');{SCRIPTS[scenario]}</script>\n"
    )


def rgba_hash(png: bytes) -> str:
    from PIL import Image

    with Image.open(io.BytesIO(png)) as image:
        rgba = image.convert("RGBA")
        if rgba.size != (WIDTH, HEIGHT):
            return f"size-{rgba.size[0]}x{rgba.size[1]}"
        return hashlib.sha256(rgba.tobytes()).hexdigest()[:16]


class Browser:
    """One fresh Chromium with a CDP connection to a page target."""

    def __init__(self, harness, chrome_dir: Path, extra_flags: list[str], begin_frame: bool):
        self.harness = harness
        self.profile = tempfile.TemporaryDirectory(prefix="q13-profile-")
        cmd = [
            str(chrome_dir / "chrome"),
            "--headless",
            "--disable-gpu",
            "--no-sandbox",
            "--no-first-run",
            "--no-default-browser-check",
            "--remote-debugging-port=0",
            f"--user-data-dir={self.profile.name}",
            f"--window-size={WIDTH},{HEIGHT + 87}",
            *extra_flags,
            "about:blank",
        ]
        self.process = subprocess.Popen(
            cmd,
            env=harness.chrome_environment(str(chrome_dir)),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
        self.clients = []
        port = harness._wait_for_devtools_endpoint(self.profile.name, self.process)
        if begin_frame:
            with open(os.path.join(self.profile.name, "DevToolsActivePort")) as source:
                browser_path = source.read().splitlines()[1]
            browser = self._connect(f"ws://127.0.0.1:{port}{browser_path}")
            target = browser.command(
                "Target.createTarget",
                {"url": "about:blank", "enableBeginFrameControl": True},
            )["targetId"]
            self.page = self._connect(f"ws://127.0.0.1:{port}/devtools/page/{target}")
        else:
            self.page = self._connect(self.harness._page_websocket_url(port))
        self.page.command("Page.enable")
        self.page.command(
            "Emulation.setDeviceMetricsOverride",
            {
                "width": WIDTH,
                "height": HEIGHT,
                "deviceScaleFactor": 1.0,
                "mobile": False,
                "screenWidth": WIDTH,
                "screenHeight": HEIGHT,
            },
        )

    def _connect(self, url: str):
        client = EventClient(url, timeout=CDP_TIMEOUT)
        self.clients.append(client)
        return client

    def navigate(self, html_file: Path) -> None:
        self.page.command("Page.navigate", {"url": "file://" + str(html_file)})

    def wait_loaded(self, frame=None) -> None:
        """Poll readiness without rAF, which may not run under the method."""
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            if frame is not None:
                frame()
            state = self.evaluate("document.readyState + ' ' + document.fonts.status")
            if state == "complete loaded":
                return
            if frame is None:
                time.sleep(0.02)
        raise TimeoutError("page did not load")

    def evaluate(self, expression: str, await_promise: bool = False):
        result = self.page.command(
            "Runtime.evaluate",
            {"expression": expression, "awaitPromise": await_promise, "returnByValue": True},
        )
        if "exceptionDetails" in result:
            raise RuntimeError(f"evaluate failed: {result['exceptionDetails']}")
        return result.get("result", {}).get("value")

    def screenshot(self) -> bytes:
        result = self.page.command(
            "Page.captureScreenshot",
            {"format": "png", "fromSurface": True, "captureBeyondViewport": False},
        )
        return base64.b64decode(result["data"])

    def probe(self):
        return json.loads(self.evaluate(PROBE))

    def close(self) -> None:
        for client in self.clients:
            self.harness._close_chromium_client(client)
        self.harness._stop_chromium_process(self.process)
        self.profile.cleanup()


def make_event_client(harness):
    class Client(harness._CdpWebSocket):
        """The harness client, keeping events instead of dropping them."""

        def __init__(self, url, timeout):
            super().__init__(url, timeout=timeout)
            self.events = []

        def command(self, method, params=None):
            command_id = self.next_id
            self.next_id += 1
            request = {"id": command_id, "method": method}
            if params is not None:
                request["params"] = params
            self._send_frame(json.dumps(request, separators=(",", ":")).encode("utf-8"))
            while True:
                message = json.loads(self._read_message())
                if "method" in message and "id" not in message:
                    self.events.append(message)
                    continue
                if message.get("id") != command_id:
                    continue
                if "error" in message:
                    raise RuntimeError(f"CDP {method} failed: {message['error']}")
                return message.get("result", {})

        def wait_event(self, method):
            while True:
                for index, event in enumerate(self.events):
                    if event["method"] == method:
                        return self.events.pop(index)
                message = json.loads(self._read_message())
                if "method" in message and "id" not in message:
                    self.events.append(message)

    return Client


EventClient = None


def capture_paused(browser: Browser, scenario: str, pages: Path):
    """Seek a paused animation to each t; one navigation per t."""
    samples = {}
    for t in TIMES:
        page = pages / f"{scenario}-paused-{t}.html"
        page.write_text(page_html(scenario, t if scenario == "css" else None))
        browser.navigate(page)
        browser.wait_loaded()
        if scenario == "waapi":
            browser.evaluate(
                f"start(); anim.pause(); anim.currentTime = {t};"
            )
        browser.evaluate(
            "new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))",
            await_promise=True,
        )
        samples[t] = (browser.screenshot(), browser.probe())
    return samples


def capture_virtual_time(browser: Browser, scenario: str, pages: Path):
    page = pages / f"{scenario}.html"
    page.write_text(page_html(scenario))
    # Pausing before navigation stalls the load (measured), so load on real
    # time, then pause virtual time before the animation starts.
    browser.navigate(page)
    browser.wait_loaded()
    browser.page.command("Emulation.setVirtualTimePolicy", {"policy": "pause"})
    browser.evaluate("start()")
    samples = {}
    now = 0
    for t in TIMES:
        if t > now:
            browser.page.command(
                "Emulation.setVirtualTimePolicy", {"policy": "advance", "budget": t - now}
            )
            browser.page.wait_event("Emulation.virtualTimeBudgetExpired")
            now = t
        samples[t] = (browser.screenshot(), browser.probe())
    return samples


def capture_begin_frame(browser: Browser, scenario: str, pages: Path):
    page = pages / f"{scenario}.html"
    page.write_text(page_html(scenario))
    clock = {"ticks": 1000.0}

    def frame(at=None, screenshot=False):
        clock["ticks"] = at if at is not None else clock["ticks"] + FRAME_MS
        params = {"frameTimeTicks": clock["ticks"], "interval": FRAME_MS}
        if screenshot:
            params["screenshot"] = {"format": "png"}
        return browser.page.command("HeadlessExperimental.beginFrame", params)

    browser.navigate(page)
    browser.wait_loaded(frame=frame)
    frame()
    browser.evaluate("start()")
    start = clock["ticks"] + FRAME_MS
    samples = {}
    for t in TIMES:
        result = frame(at=start + t, screenshot=True)
        data = result.get("screenshotData")
        if data is None:
            raise RuntimeError(f"no screenshot at t={t}: {result}")
        samples[t] = (base64.b64decode(data), browser.probe())
    return samples


def capture_raf_pump(browser: Browser, scenario: str, pages: Path):
    """Call the page's rAF callbacks at 60 Hz frame times up to each t."""
    page = pages / f"{scenario}-pump.html"
    page.write_text(page_html(scenario, raf_pump=True))
    browser.navigate(page)
    browser.wait_loaded()
    if WARM_UP:
        # The first capture of a new surface can be stale (measured: 4 of 10
        # without this; Div's harness confirms captures for the same reason).
        browser.screenshot()
    start = 1000.0
    browser.evaluate(f"start(); __pump({start})")
    samples = {}
    frame = 0
    for t in TIMES:
        while (frame + 1) * FRAME_MS < t:
            frame += 1
            browser.evaluate(f"__pump({start + frame * FRAME_MS})")
        if t > 0:
            browser.evaluate(f"__pump({start + t})")
        samples[t] = (browser.screenshot(), browser.probe())
    return samples


CAPTURE = {
    "paused": (capture_paused, [], False),
    "virtual-time": (capture_virtual_time, [], False),
    "virtual-time-main": (capture_virtual_time, MAIN_THREAD_FLAGS, False),
    "begin-frame": (
        capture_begin_frame,
        ["--deterministic-mode", "--enable-begin-frame-control"],
        True,
    ),
    "raf-pump": (capture_raf_pump, MAIN_THREAD_FLAGS, False),
}


def main() -> int:
    global EventClient
    div_checkout = Path(sys.argv[1]).resolve()
    chrome_dir = Path(sys.argv[2]).resolve()
    runs = int(sys.argv[3]) if len(sys.argv) > 3 else 10
    sys.path.insert(0, str(div_checkout / "tools" / "accountability"))
    import run_all_pixel_comparisons as harness  # noqa: E402

    EventClient = make_event_client(harness)
    pages = OUT / "html"
    pages.mkdir(parents=True, exist_ok=True)
    (OUT / "png").mkdir(parents=True, exist_ok=True)

    results = {}
    for method in METHODS:
        function, flags, begin_frame = CAPTURE[method]
        for scenario in SCENARIOS:
            # A rAF loop cannot be seeked; the pump applies only to rAF.
            if (method == "paused") and scenario == "raf":
                continue
            if method == "raf-pump" and scenario != "raf":
                continue
            key = f"{method}/{scenario}"
            results[key] = {"runs": [], "errors": []}
            for run in range(runs):
                browser = None
                try:
                    browser = Browser(harness, chrome_dir, flags, begin_frame)
                    samples = function(browser, scenario, pages)
                    record = {}
                    for t, (png, probe) in samples.items():
                        record[str(t)] = {
                            "rgba": rgba_hash(png),
                            "png": hashlib.sha256(png).hexdigest()[:16],
                            "probe": probe,
                        }
                        if run == 0:
                            (OUT / "png" / f"{method}-{scenario}-{t}.png").write_bytes(png)
                    results[key]["runs"].append(record)
                except Exception as error:  # noqa: BLE001 - recorded, not hidden
                    message = f"run {run}: {type(error).__name__}: {error}"
                    results[key]["errors"].append(message[:400])
                    if run == 0:
                        traceback.print_exc()
                finally:
                    if browser is not None:
                        browser.close()
            print(f"{key}: {len(results[key]['runs'])} runs, "
                  f"{len(results[key]['errors'])} errors", file=sys.stderr)

    at_reported = reference_at_reported_times(harness, chrome_dir, results, pages)
    (OUT / "results.json").write_text(
        json.dumps({"runs": runs, "results": results, "reference_at_reported": at_reported},
                   indent=1)
    )
    report(results, runs, at_reported)
    return 0


def reported_time(scenario: str, probe: dict):
    value = probe.get("raf" if scenario == "raf" else "animation")
    return None if value is None else round(float(value), 6)


def reference_at_reported_times(harness, chrome_dir, results, pages):
    """Paused CSS captures at every animation time a page reported."""
    times = set()
    for key, data in results.items():
        scenario = key.split("/")[1]
        for run in data["runs"]:
            for sample in run.values():
                value = reported_time(scenario, sample["probe"])
                if value is not None:
                    times.add(value)
    hashes = {}
    browser = Browser(harness, chrome_dir, [], False)
    try:
        for index, value in enumerate(sorted(times)):
            page = pages / f"reference-{index}.html"
            page.write_text(page_html("css", value))
            browser.navigate(page)
            browser.wait_loaded()
            browser.evaluate(
                "new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))",
                await_promise=True,
            )
            hashes[f"{value:.6f}"] = rgba_hash(browser.screenshot())
    finally:
        browser.close()
    return hashes


def time_range(values):
    numbers = [v for v in values if isinstance(v, (int, float))]
    if not numbers:
        return "—"
    low, high = min(numbers), max(numbers)
    return f"{low:.1f}" if abs(high - low) < 0.05 else f"{low:.1f}–{high:.1f}"


def report(results, runs, at_reported):
    reference = {}
    css = results.get("paused/css", {}).get("runs", [])
    if css:
        reference = {t: sample["rgba"] for t, sample in css[0].items()}
    print(f"Runs per row: {runs} fresh browser launches.\n")
    print("| method | scenario | t (ms) | captures | distinct RGBA | distinct PNG | "
          "reported animation time (ms) | equals nominal-t reference | "
          "equals reference at reported time |")
    print("| --- | --- | ---: | ---: | ---: | ---: | --- | --- | --- |")
    for key, data in results.items():
        method, scenario = key.split("/")
        if not data["runs"]:
            first_error = data["errors"][0] if data["errors"] else "no runs"
            print(f"| {method} | {scenario} | all | 0 | — | — | — | — | {first_error} |")
            continue
        for t in TIMES:
            samples = [run[str(t)] for run in data["runs"] if str(t) in run]
            rgba = {s["rgba"] for s in samples}
            png = {s["png"] for s in samples}
            reported = [reported_time(scenario, s["probe"]) for s in samples]
            observed = time_range(reported)
            nominal = sum(1 for s in samples if s["rgba"] == reference.get(str(t)))
            own = sum(
                1
                for s, value in zip(samples, reported)
                if value is not None and s["rgba"] == at_reported.get(f"{value:.6f}")
            )
            print(f"| {method} | {scenario} | {t} | {len(samples)} | {len(rgba)} | "
                  f"{len(png)} | {observed} | {nominal}/{len(samples)} | "
                  f"{own}/{len(samples)} |")
    for key, data in results.items():
        for error in data["errors"][:3]:
            print(f"\n- {key}: {error}")


if __name__ == "__main__":
    sys.exit(main())
