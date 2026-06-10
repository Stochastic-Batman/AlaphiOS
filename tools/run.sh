#!/usr/bin/env bash
set -euo pipefail

# Resolve paths from the script's own location so this works regardless of
# the directory `cargo run` is invoked from.
KERNEL_BIN="$(realpath "$1")"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"   # .../tools
WORKSPACE_ROOT="$(dirname "$SCRIPT_DIR")"      # .../alaphios

# Use the CARGO variable if cargo set it (avoids PATH ambiguity on some setups).
CARGO="${CARGO:-cargo}"

# Build image-builder for the host and run it with the kernel binary.
# --target-dir keeps image-builder artifacts in target/image-builder/ so
# they are cleaned by `cargo clean` and do not collide with kernel artifacts.
exec "$CARGO" run \
    --manifest-path "$WORKSPACE_ROOT/tools/image-builder/Cargo.toml" \
    --target-dir "$WORKSPACE_ROOT/target/image-builder" \
    -- "$KERNEL_BIN"
