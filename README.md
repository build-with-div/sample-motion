# Sample Motion

A dogfood sample for animation and motion on
[Div](https://github.com/build-with-div/div), a native Rust UI framework.
It ports the npm packages professionals use for motion (Motion, GSAP,
react-spring, AutoAnimate, tailwindcss-animate with Radix `Presence`, and
lottie-web) to Div's public API. Along the way it reports which web
platform features Div must implement, matching Chromium, to power them.

Status: phase 1, investigation. The repository holds probes and
measurements; the ports come next.

- [docs/design.md](docs/design.md): packages → platform features → what Div
  has and is missing, with owners, sizes, measurements and the fidelity
  plan.
- [docs/rfc-motion.md](docs/rfc-motion.md): the proposed platform work, as a
  draft for the Div tracks.
- [docs/framework-gaps.md](docs/framework-gaps.md): temporary scaffolds and
  the platform features that replace them.

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
```
