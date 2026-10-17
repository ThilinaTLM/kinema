#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
# Do not change the project toolchain or lockfile. Install tools separately:
# cargo install wasm-bindgen-cli --version 0.2.129 --locked --root /tmp/kinema-ui-probe-tools
bindgen="${KINEMA_WASM_BINDGEN:-/tmp/kinema-ui-probe-tools/bin/wasm-bindgen}"
out="${KINEMA_UI_PROBE_BUILD_DIR:-target/ui-probe/web}"
[[ "$("$bindgen" --version)" == 'wasm-bindgen 0.2.129' ]] || { echo 'Requires wasm-bindgen-cli 0.2.129' >&2; exit 1; }
cargo build -p kinema-desktop --lib --no-default-features --features ui-probe --target wasm32-unknown-unknown --locked
mkdir -p "$out"
"$bindgen" target/wasm32-unknown-unknown/debug/kinema_desktop.wasm --target web --out-dir "$out" --out-name kinema_desktop
cp scripts/ui-probe/index.html scripts/ui-probe/loader.js "$out/"
printf 'Browser bundle: %s\n' "$out"
