#!/bin/bash
# Verify that two fully isolated WASM builds produce identical SHA-256 digests.
# Implements the reproducible build requirement from docs/governance-and-security.md.

set -euo pipefail

WASM_TARGET=wasm32-unknown-unknown
WASM_REL=target/$WASM_TARGET/release/anchorkit.wasm

# ── Toolchain validation ──────────────────────────────────────────────────────
TOOLCHAIN_FILE=""
if [ -f rust-toolchain.toml ]; then
    TOOLCHAIN_FILE=rust-toolchain.toml
elif [ -f rust-toolchain ]; then
    TOOLCHAIN_FILE=rust-toolchain
fi

if [ -n "$TOOLCHAIN_FILE" ]; then
    CHANNEL=$(grep 'channel' "$TOOLCHAIN_FILE" | head -1 | cut -d'"' -f2)
    if [ -n "$CHANNEL" ]; then
        echo "Toolchain channel: $CHANNEL"
        if ! rustup toolchain list | grep -q "$CHANNEL"; then
            echo "ERROR: Required toolchain '$CHANNEL' is not installed." >&2
            echo "Run: rustup toolchain install $CHANNEL" >&2
            exit 1
        fi
    fi
fi

# ── Temp dir setup with cleanup ───────────────────────────────────────────────
BUILD_DIR_A=$(mktemp -d)
BUILD_DIR_B=$(mktemp -d)

cleanup() {
    rm -rf "$BUILD_DIR_A" "$BUILD_DIR_B"
}
trap cleanup EXIT

build_wasm() {
    local dir="$1"
    local label="$2"
    echo ""
    echo "=== Build $label (dir: $dir) ==="
    export CARGO_HOME="$dir/cargo_home"
    export RUSTUP_HOME="$dir/rustup_home"
    rsync -a --exclude target --exclude .git . "$dir/src/"
    (
        cd "$dir/src"
        cargo build --release \
            --target "$WASM_TARGET" \
            --no-default-features \
            --features wasm \
            2>&1
    )
    sha256sum "$dir/src/$WASM_REL" | awk '{print $1}'
}

echo "=== Reproducible Build Verification ==="

HASH_A=$(build_wasm "$BUILD_DIR_A" "A")
echo "Build A: $HASH_A"

HASH_B=$(build_wasm "$BUILD_DIR_B" "B")
echo "Build B: $HASH_B"

# ── Compare ───────────────────────────────────────────────────────────────────
echo ""
if [ "$HASH_A" = "$HASH_B" ]; then
    echo "✅ PASS: Both builds produce identical WASM (sha256: $HASH_A)"
else
    echo "❌ FAIL: Build outputs differ!" >&2
    echo "   Build A: $HASH_A" >&2
    echo "   Build B: $HASH_B" >&2
    exit 1
fi

# ── Optional wasm-opt pass ────────────────────────────────────────────────────
if command -v wasm-opt >/dev/null 2>&1; then
    echo ""
    echo "=== wasm-opt -Oz comparison ==="
    OPT_A="$BUILD_DIR_A/opt.wasm"
    OPT_B="$BUILD_DIR_B/opt.wasm"
    wasm-opt -Oz "$BUILD_DIR_A/src/$WASM_REL" -o "$OPT_A"
    wasm-opt -Oz "$BUILD_DIR_B/src/$WASM_REL" -o "$OPT_B"
    OHASH_A=$(sha256sum "$OPT_A" | awk '{print $1}')
    OHASH_B=$(sha256sum "$OPT_B" | awk '{print $1}')
    if [ "$OHASH_A" = "$OHASH_B" ]; then
        echo "✅ PASS: Optimized outputs also match (sha256: $OHASH_A)"
    else
        echo "⚠ WARNING: wasm-opt outputs differ (pre-opt matched)" >&2
        echo "   Opt A: $OHASH_A" >&2
        echo "   Opt B: $OHASH_B" >&2
    fi
fi

exit 0
