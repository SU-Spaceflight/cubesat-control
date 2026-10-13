I'm writing this proof of concept assuming that all the systems are running on a single raspberry pi.

Can be run with `cargo run --release` for normal operation, or `cargo run --features "save_debug_frames" -r` to save the CV output frames to the `debug_frames` directory, where they can be stitched together using something like ffmpeg: `ffmpeg -framerate 30 -i debug_frames/frame_%05d.jpg -c:v libx264 -pix_fmt yuv420p output.mp4`.

The environment variable RUST_LOG can be used to set the output logging level (e.g. `RUST_LOG=trace cargo run -r`). The file logging level is currently hardcoded to debug.

See additional implementation details in SPEC.md
