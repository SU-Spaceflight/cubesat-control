#!/usr/bin/env bash
set -e

cd "$(dirname "${BASH_SOURCE[0]}")/.."
PROJECT_ROOT=$(pwd)

echo "==> Ensuring cross is installed..."
if ! command -v cross &> /dev/null; then
    echo "Cross is not installed"
    exit 1
fi

echo "==> Building aarch64 release..."
cross build --target aarch64-unknown-linux-gnu --release --features "old_cv enable_hardware"

echo "==> Extracting and slimming libraries..."
OUT_DIR="$PROJECT_ROOT/deploy/aarch-release"
TMP_LIB="$PROJECT_ROOT/deploy/tmp-lib"

mkdir -p "$OUT_DIR/lib" "$OUT_DIR/config"
mkdir -p "$TMP_LIB"

docker run --rm -v "$TMP_LIB:/out" cubesat-cross-image bash -c " \
    find / -name '*.so*' 2>/dev/null | while read lib; do \
        if file -L \"\$lib\" 2>/dev/null | grep -q 'aarch64'; then \
            cp -L \"\$lib\" /out/ 2>/dev/null || true; \
        fi; \
    done; \
    rm -f /out/libc.so* /out/libm.so* /out/libpthread.so* /out/libdl.so* /out/librt.so* /out/libresolv.so* /out/ld-linux-*.so*"

cp target/aarch64-unknown-linux-gnu/release/cubesat-control "$OUT_DIR/"

docker run --rm \
    -v "$TMP_LIB:/tmp-lib" \
    -v "$OUT_DIR:/artifact" \
    cubesat-cross-image bash -c '
    # Function to get direct dependencies using readelf
    function get_deps() {
        aarch64-linux-gnu-readelf -d "$1" 2>/dev/null | grep NEEDED | sed -E "s/.*\[(.*)\].*/\1/" || \
        readelf -d "$1" 2>/dev/null | grep NEEDED | sed -E "s/.*\[(.*)\].*/\1/"
    }

    declare -A SEEN
    QUEUE=("/artifact/cubesat-control")

    # Breadth-first search to find all recursive dependencies
    while [ ${#QUEUE[@]} -gt 0 ]; do
        curr="${QUEUE[0]}"
        QUEUE=("${QUEUE[@]:1}")

        for dep in $(get_deps "$curr"); do
            if [ -z "${SEEN[$dep]}" ]; then
                SEEN[$dep]=1
                if [ -f "/tmp-lib/$dep" ]; then
                    cp "/tmp-lib/$dep" /artifact/lib/
                    QUEUE+=("/artifact/lib/$dep")
                fi
            fi
        done
    done

    strip --strip-unneeded /artifact/lib/*.so* 2>/dev/null || true'

echo "==> Packaging configurations and runner..."
cp config/config.toml "$OUT_DIR/config/"

cat << 'EOF' > "$OUT_DIR/start.sh"
#!/usr/bin/env bash
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" &> /dev/null && pwd)"
export LD_LIBRARY_PATH="$DIR/lib:$LD_LIBRARY_PATH"
exec "$DIR/cubesat-control" "$@"
EOF

chmod +x "$OUT_DIR/start.sh"

rm -rf "$TMP_LIB"

echo "==> Build complete! Artifacts located in $OUT_DIR"
