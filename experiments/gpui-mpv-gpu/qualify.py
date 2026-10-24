#!/usr/bin/env python3
"""Opt-in Linux VAAPI experiment qualification; no production settings or inputs."""
import argparse
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SOURCE = Path(__file__).resolve().parent
MARKER = ".kinema-experiment-owned"
PARENT_TOKEN = "kinema-gpui-mpv-gpu-v1\n"
TOKEN = "kinema-gpui-mpv-gpu-qualification-v1\n"
CASES = {
    "h2648": ("h264", "libx264", "yuv420p"),
    "hevc8": ("hevc", "libx265", "yuv420p"),
    "hevc10": ("hevc", "libx265", "yuv420p10le"),
}
# Exact key/value tokens only; never echo arbitrary backend text into reports.
FIELDS = {
    "request_hwdec": ("request_hwdec", "requested_hwdec"),
    "actual_decoder": ("actual_decoder", "hwdec-current", "hwdec_current"),
    "codec": ("codec",),
    "decoder_pixel_format": ("decoder_pixel_format",),
    "hw_pixel_format": ("hw_pixel_format",),
    "source_width": ("source_width",),
    "source_height": ("source_height",),
    "render_width": ("render_width",),
    "render_height": ("render_height",),
    "frames": ("accepted_gpu_surfaces",),
    "maps": ("cpu_pixel_maps",),
    "gpu_errors": ("gpu_validation_errors",),
    "resize_requests": ("resize_requests",),
    "resize_observed_changes": ("resize_observed_changes",),
}
NUMERIC = set(FIELDS) - {"request_hwdec", "actual_decoder", "codec",
                         "decoder_pixel_format", "hw_pixel_format"}


class GateError(ValueError):
    """Only fixed, reviewed gate messages may be included in terminal output."""


def failure_code(error):
    if isinstance(error, GateError):
        return re.sub(r"[^a-z0-9_]+", "_", str(error).lower()).strip("_")
    if isinstance(error, subprocess.TimeoutExpired):
        return "subprocess_timeout"
    if isinstance(error, FileNotFoundError):
        return "required_component_unavailable"
    if isinstance(error, OSError):
        return "subprocess_or_filesystem_failure"
    return "invalid_metadata"


def owned_directory(path, token=TOKEN):
    """Refuse links at every ancestor and never adopt an existing directory."""
    path = Path(path).absolute()
    for ancestor in (path, *path.parents):
        if ancestor.is_symlink():
            raise GateError("symlink in qualification path")
    if path.exists():
        marker = path / MARKER
        if (not path.is_dir() or marker.is_symlink() or not marker.is_file()
                or marker.read_text() != token):
            raise GateError("refusing unowned qualification directory")
    else:
        if not path.parent.is_dir():
            raise GateError("qualification parent must already exist")
        path.mkdir()
        (path / MARKER).write_text(token)
    return path


def parse_summary(text):
    tokens = re.findall(r"(?<![\w-])([A-Za-z_][A-Za-z0-9_-]*)=([^\s]+)", text)
    result = {}
    for key, aliases in FIELDS.items():
        values = {value for name, value in tokens if name in aliases}
        if len(values) != 1:
            raise GateError(f"missing or conflicting summary field: {key}")
        value = values.pop()
        if key in NUMERIC:
            if not re.fullmatch(r"[0-9]{1,12}", value):
                raise GateError(f"invalid numeric summary field: {key}")
            value = int(value)
        elif not re.fullmatch(r"[A-Za-z0-9_-]{1,64}", value):
            raise GateError(f"invalid summary field: {key}")
        result[key] = value
    output_formats = {value for name, value in tokens if name == "output_pixel_format"}
    if output_formats:
        if output_formats != {"vaapi"}:
            raise GateError("output_pixel_format must be vaapi when reported")
        result["output_pixel_format"] = "vaapi"
    if any(name == "error" and value != "false" for name, value in tokens):
        raise GateError("renderer reported failure")
    return result


def native_diagnostics(text):
    """Native libmpv/FFmpeg diagnostics can bypass the Rust log sink."""
    return {
        "native_vuid_tokens": len(re.findall(r"VUID-", text, re.IGNORECASE)),
        "native_validation_errors": len(re.findall(r"Validation\s+Error", text, re.IGNORECASE)),
    }


def validate_summary(summary, codec, size, bit_depth=8):
    expected = {
        "request_hwdec": "vaapi", "actual_decoder": "vaapi", "codec": codec,
        "decoder_pixel_format": "vaapi",
        "hw_pixel_format": "p010" if bit_depth == 10 else "nv12",
        "maps": 0, "gpu_errors": 0,
        "source_width": size[0], "source_height": size[1],
        "render_width": size[0], "render_height": size[1],
    }
    for key, value in expected.items():
        if summary[key] != value:
            raise GateError(f"qualification mismatch: {key}")
    if summary["frames"] < 3:
        raise GateError("fewer than three accepted GPU surfaces")
    if summary["resize_requests"] < 2:
        raise GateError("insufficient resize stress requests")
    if summary["resize_observed_changes"] < 2:
        raise GateError("insufficient observed resize stress changes")


def capture(argv, log, timeout, env=None):
    """Exclusive owned log, finite process-tree lifetime, no shell."""
    with log.open("xb") as output:
        process = subprocess.Popen(argv, cwd=ROOT, env=env, stdout=output,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        try:
            code = process.wait(timeout=timeout)
        except BaseException:
            # Kill descendants too (cargo, ffmpeg, native renderer).
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
            raise
    if code:
        raise GateError(f"subprocess failed ({code}); see {log.name}")


def observed_evidence(text):
    """Partial diagnostics only: never an alternative acceptance parser."""
    tokens = re.findall(r"(?<![\w-])([A-Za-z_][A-Za-z0-9_-]*)=([^\s]+)", text)
    evidence = {}
    for key, aliases in FIELDS.items():
        values = {value for name, value in tokens if name in aliases}
        if len(values) != 1:
            continue
        value = values.pop()
        if key in NUMERIC:
            if not re.fullmatch(r"[0-9]{1,12}", value):
                continue
            value = int(value)
        elif not re.fullmatch(r"[A-Za-z0-9_-]{1,64}", value):
            continue
        evidence["gpu_validation_errors" if key == "gpu_errors" else key] = value
    output_formats = {value for name, value in tokens if name == "output_pixel_format"}
    if len(output_formats) == 1:
        value = output_formats.pop()
        if re.fullmatch(r"[A-Za-z0-9_-]{1,64}", value):
            evidence["output_pixel_format"] = value
    return evidence


def capture_renderer(argv, row, log, timeout, env=None):
    try:
        capture(argv, log, timeout, env)
    finally:
        # Evidence extraction must not replace a child exit/timeout failure.
        try:
            text = log.read_text(errors="replace")
        except OSError:
            text = ""
        row.update(native_diagnostics(text))
        row["observed_evidence"] = observed_evidence(text)
    return text


def fixture_command(case, destination, size, seconds):
    _, encoder, pixel_format = CASES[case]
    width, height = size
    # Explicit limited-range BT.709 SDR, static top-red/bottom-blue halves.
    # Static white marker stays in the center, away from corner pixel probes.
    pattern = (f"color=c=blue:s={width}x{height}:r=24,"
               "drawbox=x=0:y=0:w=iw:h=ih/2:color=red:t=fill,"
               "drawbox=x=iw/2:y=ih/2-16:w=32:h=32:color=white:t=fill,"
               "scale=in_color_matrix=bt601:out_color_matrix=bt709:in_range=tv:out_range=tv,"
               f"format={pixel_format},"
               "setparams=range=limited:color_primaries=bt709:color_trc=bt709:colorspace=bt709")
    args = ["ffmpeg", "-hide_banner", "-loglevel", "error", "-nostdin", "-n",
            "-filter_threads", "2", "-f", "lavfi", "-i", pattern,
            "-t", str(seconds + 2), "-an", "-c:v", encoder, "-preset", "ultrafast",
            "-threads", "2", "-pix_fmt", pixel_format, "-color_range", "tv",
            "-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709"]
    if encoder == "libx265":
        args += ["-x265-params", "pools=2:frame-threads=2:log-level=error"]
    else:
        args += ["-profile:v", "high", "-x264-params", "cabac=1:8x8dct=1"]
    return args + [str(destination)]


def probe_fixture(media, log, codec, pixel_format, size):
    capture(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
             "stream=codec_name,pix_fmt,width,height,r_frame_rate,color_space,color_transfer,color_primaries",
             "-of", "json", str(media)], log, 20)
    streams = json.loads(log.read_text())["streams"]
    if len(streams) != 1:
        raise GateError("fixture must contain one video stream")
    expected = dict(codec_name=codec, pix_fmt=pixel_format, width=size[0], height=size[1],
                    r_frame_rate="24/1", color_space="bt709", color_transfer="bt709",
                    color_primaries="bt709")
    if any(streams[0].get(key) != value for key, value in expected.items()):
        raise GateError("fixture metadata mismatch")
    return streams[0]


def run(args):
    if sys.platform != "linux":
        raise GateError("qualification requires Linux")
    inherited = os.environ.get("KINEMA_GPU_EXPERIMENT_HWDEC")
    if inherited not in (None, "no"):
        raise GateError("unset KINEMA_GPU_EXPERIMENT_HWDEC or set it to no")
    env = os.environ.copy()
    env.pop("KINEMA_GPU_EXPERIMENT_HWDEC", None)
    env["VK_INSTANCE_LAYERS"] = "VK_LAYER_KHRONOS_validation"
    target = ROOT / "target"
    if target.is_symlink() or not target.is_dir():
        raise GateError("target must be an existing non-symlink directory")
    parent = owned_directory(target / "gpui-mpv-gpu", PARENT_TOKEN)
    base = owned_directory(parent / "qualification")
    # Never reuse or remove fixtures/logs, even from earlier owned runs.
    directory = Path(tempfile.mkdtemp(prefix="run-", dir=base))
    (directory / MARKER).write_text(TOKEN)
    report = {"status": "failed", "pixel_tests": "separate-required",
              "validation_layer_requested": env["VK_INSTANCE_LAYERS"],
              "stress_resize": True, "cases": []}
    print(f"Evidence: {directory.relative_to(ROOT)}", flush=True)
    try:
        for name, command in (("ffmpeg", ["ffmpeg", "-version"]),
                              ("mpv", ["mpv", "--version"]),
                              ("vainfo", ["vainfo", "--display", "drm", "--device", "/dev/dri/renderD128"])):
            try:
                capture(command, directory / f"{name}-caps.log", 20, env)
                report[name + "_probe"] = "ok"
            except (OSError, ValueError, subprocess.TimeoutExpired):
                report[name + "_probe"] = "unavailable-see-log"
        size = (3840, 2160) if args.four_k else (1920, 1080)
        for case, (codec, _, pixel_format) in CASES.items():
            row = {"case": case, "status": "failed"}
            report["cases"].append(row)
            media = directory / f"{case}.mkv"
            capture(fixture_command(case, media, size, args.seconds),
                    directory / f"{case}-encode.log", 180, env)
            row["fixture"] = probe_fixture(media, directory / f"{case}-probe.json",
                                           codec, pixel_format, size)
            log = directory / f"{case}-renderer.log"
            native_text = capture_renderer(
                [str(SOURCE / "run.sh"), "run", "--media", str(media),
                 "--seconds", str(args.seconds), "--render-size", f"{size[0]}x{size[1]}",
                 "--hwdec", "vaapi", "--stress-resize"], row, log, args.app_timeout, env)
            if row["native_vuid_tokens"] or row["native_validation_errors"]:
                raise GateError("native_validation_diagnostic")
            summary = parse_summary(native_text)
            row["summary"] = summary
            validate_summary(summary, codec, size, 10 if case == "hevc10" else 8)
            row["status"] = "renderer-pass-pixel-tests-required"
        report["status"] = "renderer-pass-pixel-tests-required"
    except (OSError, ValueError, KeyError, subprocess.TimeoutExpired) as error:
        report["failure_code"] = failure_code(error)
        raise
    finally:
        with (directory / "report.json").open("x") as output:
            json.dump(report, output, indent=2)
            output.write("\n")
    print("Renderer matrix passed; native pixel tests still required for qualification.")


class Tests(unittest.TestCase):
    def summary(self):
        return ("producer: request_hwdec=vaapi hwdec-current=vaapi codec=h264 decoder_pixel_format=vaapi hw_pixel_format=nv12 "
                "source_width=1920 source_height=1080 render_width=1920 render_height=1080 "
                "cpu_pixel_maps=0\nGPU experiment: accepted_gpu_surfaces=4 error=false "
                "gpu_validation_errors=0 resize_requests=3 resize_observed_changes=2")

    def test_success_and_alias(self):
        for text in (self.summary(), self.summary().replace("hwdec-current", "actual_decoder")):
            validate_summary(parse_summary(text), "h264", (1920, 1080))

    def test_native_diagnostics_and_10bit(self):
        self.assertEqual(native_diagnostics("Mesa: alternate EGL probe warning"),
                         {"native_vuid_tokens": 0, "native_validation_errors": 0})
        for message in ("VUID-vkSubmit-12345", "vuid-anything", "VALIDATION ERROR: test"):
            self.assertGreater(sum(native_diagnostics(message).values()), 0)
        text = self.summary().replace("codec=h264", "codec=hevc").replace(
            "hw_pixel_format=nv12", "hw_pixel_format=p010")
        validate_summary(parse_summary(text), "hevc", (1920, 1080), 10)
        parse_summary(text + " output_pixel_format=vaapi")
        with self.assertRaises(ValueError):
            parse_summary(text + " output_pixel_format=yuv420p")

    def test_missing_conflicting_invalid(self):
        for text in (self.summary().replace("codec=h264", ""),
                     self.summary() + " actual_decoder=vaapi-copy",
                     self.summary().replace("cpu_pixel_maps=0", "cpu_pixel_maps=-1"),
                     self.summary().replace("error=false", "error=true")):
            with self.assertRaises(ValueError):
                parse_summary(text)

    def test_rejects_fallback_and_bad_metrics(self):
        summary = parse_summary(self.summary())
        for key, value in (("actual_decoder", "vaapi-copy"), ("actual_decoder", "no"),
                           ("request_hwdec", "auto"), ("decoder_pixel_format", "yuv420p"),
                           ("hw_pixel_format", "vaapi"), ("hw_pixel_format", "p010"),
                           ("gpu_errors", 1), ("maps", 1), ("frames", 2),
                           ("source_width", 640), ("render_height", 360), ("codec", "hevc")):
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                validate_summary(dict(summary, **{key: value}), "h264", (1920, 1080))

    def test_nonzero_renderer_retains_observed_evidence(self):
        with tempfile.TemporaryDirectory() as tmp:
            row = {"case": "h2648", "status": "failed"}
            report = {"status": "failed", "cases": [row]}
            # Historical failed logs need not contain the newly required stress fields.
            text = self.summary().replace("gpu_validation_errors=0", "gpu_validation_errors=8")
            text = re.sub(r" resize_(?:requests|observed_changes)=\d+", "", text)
            text += " secret_field=not_for_report"
            try:
                capture_renderer([sys.executable, "-c", f"print({text!r}); raise SystemExit(1)"],
                                 row, Path(tmp) / "renderer.log", 5)
                self.fail("nonzero renderer must fail")
            except GateError as error:
                report["failure_code"] = failure_code(error)
            saved = json.loads(json.dumps(report))
            self.assertEqual(saved["status"], "failed")
            self.assertEqual(saved["cases"][0]["status"], "failed")
            evidence = saved["cases"][0]["observed_evidence"]
            self.assertEqual(evidence["gpu_validation_errors"], 8)
            self.assertEqual(evidence["actual_decoder"], "vaapi")
            self.assertEqual(evidence["source_width"], 1920)
            self.assertNotIn("secret_field", evidence)
            self.assertEqual(saved["failure_code"], "subprocess_failed_1_see_renderer_log")
            with self.assertRaises(GateError):
                parse_summary(text)

    def test_resize_stress_requires_observed_changes(self):
        summary = parse_summary(self.summary())
        for key in ("resize_requests", "resize_observed_changes"):
            for value in (0, 1):
                with self.subTest(key=key, value=value), self.assertRaises(GateError):
                    validate_summary(dict(summary, **{key: value}), "h264", (1920, 1080))
            with self.assertRaisesRegex(GateError, key):
                parse_summary(self.summary().replace(f"{key}={summary[key]}", ""))
        # Requests alone are not evidence of actual swapchain/window size changes.
        with self.assertRaisesRegex(GateError, "observed"):
            validate_summary(dict(summary, resize_requests=20, resize_observed_changes=0),
                             "h264", (1920, 1080))

    def test_owned_directories(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            owned = owned_directory(root / "owned")
            self.assertEqual(owned_directory(owned), owned)
            unknown = root / "unknown"
            unknown.mkdir()
            (unknown / "keep").write_text("user")
            with self.assertRaises(ValueError):
                owned_directory(unknown)
            self.assertEqual((unknown / "keep").read_text(), "user")
            (root / "link").symlink_to(owned, target_is_directory=True)
            for path in (root / "link", root / "link" / "child"):
                with self.assertRaises(ValueError):
                    owned_directory(path)
            (owned / MARKER).unlink()
            (owned / MARKER).symlink_to(unknown / "keep")
            with self.assertRaises(ValueError):
                owned_directory(owned)

    def test_exclusive_logs_and_timeout(self):
        with tempfile.TemporaryDirectory() as tmp:
            log = Path(tmp) / "existing.log"
            log.write_text("preserve")
            with self.assertRaises(FileExistsError):
                capture([sys.executable, "-c", "pass"], log, 5)
            self.assertEqual(log.read_text(), "preserve")
            with self.assertRaises(subprocess.TimeoutExpired):
                capture([sys.executable, "-c", "import time; time.sleep(30)"],
                        Path(tmp) / "timeout.log", 0.1)

    def test_fixed_failure_codes(self):
        self.assertEqual(failure_code(FileNotFoundError("/private/path")),
                         "required_component_unavailable")
        self.assertEqual(failure_code(ValueError("private backend text")), "invalid_metadata")

    def test_encoding_argument_lists(self):
        for case in CASES:
            command = fixture_command(case, Path("/owned/fixture.mkv"), (1920, 1080), 5)
            self.assertIn("-n", command)
            self.assertEqual(command[command.index("-threads") + 1], "2")
            self.assertEqual(command[-1], "/owned/fixture.mkv")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", nargs="?", choices=["run"], default="run")
    parser.add_argument("--self-test", action="store_true", help="stdlib tests only; no GPU/tools")
    parser.add_argument("--seconds", type=int, choices=range(4, 11), default=5, metavar="4..10")
    parser.add_argument("--4k", dest="four_k", action="store_true", help="3840x2160 source AND render")
    parser.add_argument("--app-timeout", type=int, choices=range(30, 901), default=300, metavar="30..900")
    args = parser.parse_args()
    if args.self_test:
        return 0 if unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(Tests)).wasSuccessful() else 1
    try:
        run(args)
    except (OSError, ValueError, KeyError, subprocess.TimeoutExpired) as error:
        print(f"Qualification FAILED: {failure_code(error)}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
