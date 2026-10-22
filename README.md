# CubeSat-Control

A proof of concept satellite controller designed to run on a Raspberry Pi compute module 4. It parses comands, sends telemetry and uses a camera feed to manipulate a gimbal.

For a detailed implementation document, refer to [docs/SPEC.md](docs/SPEC.md).

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

## Configuration

Possible configuration values are defined in [config.rs](src/config.rs). Default values are defined in [config/config.toml](config/config.toml). Each configuration value can be overridden when running the program.

For example, here is how to change the IP of the GC that the satellite tries to connect to.

```bash
SAT__NETWORK__GROUND_STATION_IP="192.168.1.100" cargo run -r
```

> [!NOTE]
> `.env` support coming soon!

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

> [!NOTE]
> I have only tested these steps on my machine as of writing this. The steps should be similar for non-NixOS machines, but may differ (maybe easier, maybe not).

For cross-compilation I am using [cross-rs](https://github.com/cross-rs/cross) which creates a docker container that the code is compiled in. In this case, it will create a debian docker environment and cross-compile within it to make use of Debians available arm64 packages. [Dockerfile.cross](Dockerfile.cross) is the used configuration file for cross-rs.

### Steps for cross-compilation

Creates the docker container, installs dependencies and compiles within it. Docker or Podman is required.

```bash
cross build --target aarch64-unknown-linux-gnu --release --features old_cv
```

Assuming that it compiled with no issues, just running the binary on the Pi likely will not work as the linked libraries are not available or are not the correct version.

Collect all the libraries (minus a few exceptions) within the docker environment and place them within ./lib:

```bash
docker run --rm -v "$PWD/lib:/out" cubesat-cross-image bash -c " \
  find / -name '*.so*' 2>/dev/null | while read lib; do \
    if file -L \"\$lib\" 2>/dev/null | grep -q 'aarch64'; then \
      cp -L \"\$lib\" /out/ 2>/dev/null || true; \
    fi; \
  done; \
  rm -f /out/libc.so* /out/libm.so* /out/libpthread.so* /out/libdl.so* /out/librt.so* /out/libresolv.so* /out/ld-linux-*.so* \
"
```

After that, copy the following to the raspberry Pi:

```
./target/aarch64-unknown-linux-gnu/release/cubesat-control
./lib
```

[rsync](https://github.com/RsyncProject/rsync) example:

```bash
rsync -avzP lib target/aarch64-unknown-linux-gnu/release/cubesat-control {IP-ADDRESS}:~/
```

This will place all the files in the Pi's home directory.

To run the program, you need to tell the program where to find the libraries, so run it with this environment variable:

```
LD_LIBRARY_PATH=./lib ./cubesat-control
```

### File size

The lib directory is quite large (>2GB for me). You can reduce the size by first stripping out all of the useless metadata and debug symbols using strip.

```bash
strip --strip-unneeded lib/*.so* 2>/dev/null || true
```

You can also remove un-needed libraries (since copying all of them leads to many that are never used being copied).

```bash
mkdir lib-slim
LD_LIBRARY_PATH=./lib ldd ./cubesat-control | grep '=> ./lib/' | awk '{print $3}' | xargs -I {} cp {} lib-slim/
```

I would recommend running the program with `LD_LIBRARY_PATH` set to `./lib-slim` to see if the program loads any libraries during runtime (which wouldn't be caught by ldd). If any are required, copy them over from `./lib`. You can then remove `./lib` and rename `./lib-slim` to `./lib`.
