# CubeSat-Control

A proof of concept satellite controller designed to run on a Raspberry Pi compute module 4. It parses comands, sends telemetry and uses a camera feed to manipulate a gimbal.

For a detailed implementation document, refer to [SPEC.md](SPEC.md).

To use the githooks provided, run `git config core.hooksPath .githooks`.

## Prerequisites & Dependencies

### Option A: Nix (Recommended)

A `flake.nix` is provided with pre-configured Rust toolchains, OpenCV, Clang, mold, and cross-compilation headers:

```bash
nix develop
```

### Option B: Native Installation

Ensure the following packages and libraries are available in your system path:

- **Rust:** Stable toolchain
- **OpenCV 4.x:** Development headers and runtime (`libopencv-dev`)
- **Clang/LLVM:** Required by `rust-bindgen` (`libclang-dev`)
- **Build tools:** `pkg-config`, `cmake`
- **Python 3.10+:** For the mock Ground Control server (`websockets`)

## Quick Start With Nix

In terminal 1:

```bash
python test_server.py
```

In terminal 2:

```bash
# Standard run
cargo run --release

# Run with increased logging verbosity
RUST_LOG=debug cargo run --release # Or
RUST_LOG=trace cargo run --release
```

Logs are mirrored in daily rotating files in `./log/flight_system.log.*` at `debug` level, while standard console output adheres to `RUST_LOG` (defaults to `info`).

## Quick Start Without Nix

The satellite client expects an active WebSocket server at `ws://127.0.0.1:8080`:

In terminal 1:

```bash
python3 -m venv .venv
source .venv/bin/activate
pip -r requirements.txt

python3 test_server.py
```

In terminal 2:

```bash
# Standard run
cargo run --release

# Run with increased logging verbosity
RUST_LOG=debug cargo run --release # Or
RUST_LOG=trace cargo run --release
```

Logs are mirrored in daily rotating files in `./log/flight_system.log.*` at `debug` level, while standard console output adheres to `RUST_LOG` (defaults to `info`).

## Debugging CV & Recording Video

To dump processed OpenCV frames with detected centroid overlays:

```bash
cargo run --release --features "save_debug_frames"
```

Render the generated frames into an MP4 video using `ffmpeg`:

```bash
ffmpeg -framerate 30 -i debug_frames/frame_%05d.jpg -c:v libx264 -pix_fmt yuv420p cv_tracking_output.mp4
```

## Cross-Compilation for Raspberry Pi 4

To cross-compile for an `aarch64-unknown-linux-gnu` target:

```bash
cargo build --target aarch64-unknown-linux-gnu --release
```
