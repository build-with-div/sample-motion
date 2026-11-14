# Sample Motion

A dogfood sample for animation and motion on
[Div](https://github.com/build-with-div/div), a native Rust UI framework.
It ports the npm packages professionals use for motion (Motion, GSAP,
react-spring, AutoAnimate, tailwindcss-animate with Radix `Presence`, and
lottie-web) to Div's public API. Along the way it reports which web
platform features Div must implement, matching Chromium, to power them.

Status: phase 2. The repository holds the probes and measurements of
phase 1. The package ports are landing one milestone at a time; see
[design.md § Phase 2](docs/design.md#phase-2-porting-the-packages).

- [docs/design.md](docs/design.md): packages → platform features → what Div
  has and is missing, with owners, sizes, measurements and the fidelity
  plan.
- [docs/rfc-motion.md](docs/rfc-motion.md): the proposed platform work, as a
  draft for the Div tracks.
- [docs/framework-gaps.md](docs/framework-gaps.md): temporary scaffolds and
  the platform features that replace them.
- [docs/motion-port.md](docs/motion-port.md): the Motion `animate()` port
  and its comparison with Motion in Chromium.
- [docs/q13-chromium-capture.md](docs/q13-chromium-capture.md): which ways
  of driving time in pinned Chromium give byte-stable captures.

## Build and test

Div is not on crates.io; `Cargo.toml` pins it to a commit of
build-with-div/div, and `Cargo.lock` is seeded from Div's own lock. Div
compiles Skia from source, which needs Rust 1.94.1 (`rust-toolchain.toml`),
Python 3, Ninja and LLVM; on Windows, Visual Studio 2022 Build Tools with
`vcvars64.bat` applied and `LIBCLANG_PATH` set; on Linux, the packages in
[.github/workflows/ci.yml](.github/workflows/ci.yml). `.cargo/config.toml`
sets Div's Skia build variables.

```sh
cargo test                                   # the API probe
cargo test --release --test measure_frame_cost -- --ignored --nocapture --test-threads 1
cargo run --release --example frame_pacing -- engine 100 5
cargo test --test fidelity_sampled -- --ignored   # then tools/chromium_sampled.py
cargo test --test motion_native              # the Motion port against Motion's recorded calls
cargo test --test motion_sequence            # stagger() and sequences against Motion's recorded calls
cargo test --test motion_pixels -- --ignored # then tools/motion_oracle.py pixels
```

Run the Chromium tools in Linux or WSL, against a build-with-div/div
checkout and its pinned Chromium:

```sh
python3 tools/chromium_sampled.py DIV_CHECKOUT CHROME_DIR
python3 tools/chromium_capture_stability.py DIV_CHECKOUT CHROME_DIR 10
python3 tools/motion_oracle.py values DIV_CHECKOUT CHROME_DIR
python3 tools/motion_oracle.py pixels DIV_CHECKOUT CHROME_DIR
python3 tools/motion_oracle.py sequences DIV_CHECKOUT CHROME_DIR
python3 tools/motion_oracle.py sequence-pixels DIV_CHECKOUT CHROME_DIR
```
