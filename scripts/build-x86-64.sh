#!/usr/bin/env bash
set -e

cd "$(dirname "${BASH_SOURCE[0]}")/.."
PROJECT_ROOT=$(pwd)

echo "Building x86_64 release..."
cargo build --release --features old_cv

echo "Packaging artifacts..."
OUT_DIR="$PROJECT_ROOT/deploy/x86-64-release"
mkdir -p "$OUT_DIR/config"

cp target/release/cubesat-control "$OUT_DIR/"
cp config/config.toml "$OUT_DIR/config/"

echo "Build complete! Artifacts located in $OUT_DIR"
