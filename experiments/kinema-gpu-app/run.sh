#!/usr/bin/env bash
# Build only the isolated generated workspace; no root patches or registry writes.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
SOURCE="$ROOT/experiments/kinema-gpu-app"
BUILD="$ROOT/target/kinema-gpu-app"
export CARGO_TARGET_DIR="$ROOT/target"
cd "$ROOT"
command="${1:-check}"
if (( $# )); then shift; fi
case "$command" in
    prepare)
        exec python3 "$SOURCE/prepare.py" "$@"
        ;;
    check)
        python3 "$SOURCE/prepare.py"
        exec cargo check --manifest-path "$BUILD/Cargo.toml" --locked \
            -p kinema-desktop --features experimental-gpu "$@"
        ;;
    build)
        python3 "$SOURCE/prepare.py"
        artifacts="$(mktemp "$BUILD/build-artifacts.XXXXXX")"
        trap 'rm -f "$artifacts"' EXIT
        cargo build --manifest-path "$BUILD/Cargo.toml" --locked \
            -p kinema-desktop --features experimental-gpu --bin kinema-desktop \
            --message-format=json-render-diagnostics "$@" >"$artifacts"
        python3 - "$artifacts" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as artifacts:
    for line in artifacts:
        record = json.loads(line)
        if (record.get("reason") == "compiler-artifact"
                and record.get("target", {}).get("name") == "kinema-desktop"
                and not record.get("profile", {}).get("test", False)
                and record.get("executable")):
            print("Built experimental app: " + record["executable"])
PY
        ;;
    test)
        python3 "$SOURCE/prepare.py"
        exec cargo test --manifest-path "$BUILD/Cargo.toml" --locked \
            --workspace --features kinema-desktop/experimental-gpu "$@"
        ;;
    run)
        if [[ "${1:-}" == --probe ]]; then
            # Agent/native validation uses only explicit offline fixtures here.
            shift
            python3 "$SOURCE/prepare.py"
            session="$(mktemp -d "$BUILD/probe-session.XXXXXX")"
            cd "$session"
            exec cargo run --quiet --manifest-path "$BUILD/Cargo.toml" --locked \
                -p kinema-desktop --features experimental-gpu,ui-probe --bin ui-probe -- "$@"
        fi
        # Explicit USER launch: ordinary app paths and OS credentials may be used.
        # Agents must never execute this branch for testing.
        python3 "$SOURCE/prepare.py"
        exec cargo run --quiet --manifest-path "$BUILD/Cargo.toml" --locked \
            -p kinema-desktop --features experimental-gpu --bin kinema-desktop -- "$@"
        ;;
    *)
        echo 'Usage: run.sh {prepare|build|check|test|run [app args]|run --probe [fixture args]}' >&2
        exit 2
        ;;
esac
