#!/usr/bin/env bash
set -euo pipefail

KERNEL_BIN="$(realpath "$1")"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKSPACE_ROOT="$(dirname "$SCRIPT_DIR")"

CARGO="${CARGO:-cargo}"

HOST_TARGET="$(rustc -vV | sed -n 's|host: ||p')"

# cd to workspace root so cargo does NOT pick up kernel/.cargo/config.toml
# (which has build-std) when compiling the host-side image-builder.
cd "$WORKSPACE_ROOT"

exec "$CARGO" run \
    --manifest-path "tools/image-builder/Cargo.toml" \
    --target-dir "target/image-builder" \
    --target "$HOST_TARGET" \
    -- "$KERNEL_BIN"
