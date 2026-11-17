#!/usr/bin/env bash
# Build the browser version of nscheme (bead nscheme-yd0.1).
#
#   web/build.sh            release build into web/www/pkg/
#   web/build.sh --dev      unoptimized, faster to build
#
# Needs the wasm32 target and a wasm-bindgen CLI of exactly the version
# web/Cargo.toml pins:
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version <that version> --locked
#
# Serve the page with `python3 -m http.server -d web/www` and open
# http://localhost:8000/. (Browsers refuse to load .wasm from file://.)
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
profile=release
cargo_flags=(--release)
if [[ "${1:-}" == "--dev" ]]; then
    profile=debug
    cargo_flags=()
fi

pinned="$(sed -n 's/^wasm-bindgen = "=\(.*\)"/\1/p' "$root/web/Cargo.toml")"
installed="$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')" || true
if [[ "$installed" != "$pinned" ]]; then
    echo "web/build.sh: need wasm-bindgen CLI $pinned, found '${installed:-none}'." >&2
    echo "  cargo install wasm-bindgen-cli --version $pinned --locked" >&2
    exit 1
fi

cargo build -p nscheme-web --target wasm32-unknown-unknown "${cargo_flags[@]+"${cargo_flags[@]}"}" \
    --manifest-path "$root/Cargo.toml"
wasm-bindgen --target web --no-typescript \
    --out-dir "$root/web/www/pkg" \
    "${CARGO_TARGET_DIR:-$root/target}/wasm32-unknown-unknown/$profile/nscheme_web.wasm"
ls -l "$root/web/www/pkg"
