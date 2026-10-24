#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
SOURCE="$ROOT/experiments/gpui-mpv-gpu"
BUILD="$ROOT/target/gpui-mpv-gpu"
cd "$ROOT"
if [[ "$(uname -s)" != Linux ]]; then
    echo 'Unsupported platform: this experiment requires Linux Vulkan.' >&2
    exit 1
fi
# Reuse root dependency artifacts, while keeping manifests/locks/patch copies isolated.
export CARGO_TARGET_DIR="$ROOT/target"
command="${1:-run}"
if (( $# )); then shift; fi
case "$command" in
    prepare) exec python3 "$SOURCE/prepare.py" "$@" ;;
    check|test)
        python3 "$SOURCE/prepare.py"
        exec cargo "$command" --manifest-path "$BUILD/Cargo.toml" --locked "$@"
        ;;
    run)
        # Keep a bounded launcher policy too, before running ffmpeg or opening files.
        seconds=10
        seconds_seen=0
        media=''
        hwdec=no
        hwdec_seen=0
        render_width=640
        render_height=360
        render_seen=0
        stress_resize=0
        while (( $# )); do
            case "$1" in
                --seconds)
                    [[ $seconds_seen -eq 0 && $# -ge 2 && "$2" =~ ^[0-9]+$ && ${#2} -le 3 ]] || { echo 'Invalid --seconds' >&2; exit 1; }
                    seconds_seen=1
                    seconds=$((10#$2))
                    (( seconds >= 1 && seconds <= 120 )) || { echo '--seconds must be 1..120' >&2; exit 1; }
                    shift 2 ;;
                --stress-resize)
                    [[ $stress_resize -eq 0 ]] || { echo 'Duplicate --stress-resize' >&2; exit 1; }
                    stress_resize=1
                    shift ;;
                --hwdec)
                    [[ $hwdec_seen -eq 0 && $# -ge 2 && ( "$2" == no || "$2" == vaapi ) ]] || { echo '--hwdec requires no or explicit vaapi (no auto/copy modes)' >&2; exit 1; }
                    hwdec_seen=1
                    hwdec="$2"
                    shift 2 ;;
                --render-size)
                    [[ $render_seen -eq 0 && $# -ge 2 && "$2" =~ ^([0-9]{1,10})x([0-9]{1,10})$ ]] || { echo '--render-size requires WIDTHxHEIGHT' >&2; exit 1; }
                    render_seen=1
                    render_width=$((10#${BASH_REMATCH[1]}))
                    render_height=$((10#${BASH_REMATCH[2]}))
                    (( render_width >= 1 && render_width <= 3840 && render_height >= 1 && render_height <= 2160 )) || { echo 'Render size exceeds 3840x2160 limits or is zero' >&2; exit 1; }
                    (( render_width * render_height * 4 <= 64 * 1024 * 1024 )) || { echo 'Render pixels exceed 64 MiB' >&2; exit 1; }
                    shift 2 ;;
                --media)
                    [[ $# -ge 2 && -z "$media" && "$2" != *://* ]] || { echo 'Expected one explicit local file' >&2; exit 1; }
                    [[ -f "$2" ]] || { echo 'Local media file is unavailable' >&2; exit 1; }
                    media="$(realpath -- "$2" 2>/dev/null)" || { echo 'Cannot resolve local media file' >&2; exit 1; }
                    shift 2 ;;
                *) echo 'Usage: run.sh {prepare|check|test|run} [--media LOCAL_FILE] [--seconds 1..120] [--hwdec no|vaapi] [--render-size WIDTHxHEIGHT] [--stress-resize]' >&2; exit 1 ;;
            esac
        done
        (( !stress_resize || seconds >= 4 )) || { echo '--stress-resize requires --seconds >= 4' >&2; exit 1; }
        stress_args=()
        if (( stress_resize )); then stress_args=(--stress-resize); fi
        python3 "$SOURCE/prepare.py"
        if [[ -z "$media" ]]; then
            mkdir -p "$BUILD/fixtures"
            media="$BUILD/fixtures/testsrc2-${hwdec}-${render_width}x${render_height}.mkv"
            codec_args=(-c:v mpeg4 -q:v 3)
            if [[ "$hwdec" == vaapi ]]; then
                (( render_width % 2 == 0 && render_height % 2 == 0 )) || { echo 'Synthetic H.264/yuv420p requires even dimensions' >&2; exit 1; }
                codec_args=(-c:v libx264 -pix_fmt yuv420p -threads 2 -preset fast -crf 18)
            fi
            # Generated synthetic content only. Codec/media arguments remain an array.
            if ! ffmpeg -hide_banner -loglevel error -nostdin -y -f lavfi \
                -i "testsrc2=size=${render_width}x${render_height}:rate=30" -t "$((seconds + 2))" \
                -an "${codec_args[@]}" "$media" 2>"$BUILD/fixture-error.log"; then
                echo 'Synthetic fixture generation failed (see target/gpui-mpv-gpu/fixture-error.log)' >&2
                exit 1
            fi
        fi
        # Backend selection is process-local, never a production default.
        export WGPU_BACKEND=vulkan
        exec cargo run --quiet --manifest-path "$BUILD/Cargo.toml" --locked -- \
            --media "$media" --seconds "$seconds" --hwdec "$hwdec" \
            --render-size "${render_width}x${render_height}" "${stress_args[@]}"
        ;;
    *) echo 'Expected prepare, check, test, or run' >&2; exit 1 ;;
esac
