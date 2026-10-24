#!/usr/bin/env python3
"""Materialize regression harness using promoted repository-owned GPU sources."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
SOURCE = Path(__file__).resolve().parent
BUILD = ROOT / "target/gpui-mpv-gpu"
MARKER = ".kinema-experiment-owned"
MARKER_VALUE = "kinema-gpui-mpv-gpu-v1\n"
PATCH_CRATES = {
    "0001-gpui-pre-0.3.9.patch": "gpui-pre",
    "0002-gpui-pre-wgpu-0.3.9.patch": "gpui-pre-wgpu",
    "0003-wgpu-hal-29.0.4.patch": "wgpu-hal",
    "0004-gpui-pre-apple-0.3.9.patch": "gpui-pre-apple",
    "0005-gpui-pre-windows-0.3.9.patch": "gpui-pre-windows",
}
PATCH_VERSIONS = {
    "gpui-pre": "0.3.9",
    "gpui-pre-wgpu": "0.3.9",
    "wgpu-hal": "29.0.4",
    "gpui-pre-apple": "0.3.9",
    "gpui-pre-windows": "0.3.9",
}


def validate_patches(patches):
    names = {path.name for path in patches}
    if names != set(PATCH_CRATES):
        raise ValueError("expected exactly five checked-in GPUI/WGPU/HAL/Apple/Windows experiment patches")
    return dict(PATCH_CRATES)


def selected_patch_crates(directory):
    return validate_patches(sorted(directory.glob("*.patch")))


def owned_directory(path, marker_value=MARKER_VALUE):
    if path.is_symlink():
        raise ValueError(f"refusing symlink directory: {path.name}")
    if path.exists():
        marker = path / MARKER
        if not marker.is_file() or marker.is_symlink() or marker.read_text() != marker_value:
            raise ValueError(f"refusing non-owned preexisting directory: {path.name}")
    else:
        path.mkdir(parents=True)
        (path / MARKER).write_text(marker_value)


def write_generated(path, text):
    if path.is_symlink() or (path.exists() and not path.is_file()):
        raise ValueError(f"refusing nonregular generated file: {path.name}")
    if not path.exists() or path.read_text() != text:
        path.write_text(text)


def verify_source(source, package):
    """Verify locked package identity and every source file before copying."""
    if source.is_symlink() or any(path.is_symlink() for path in source.rglob("*")):
        raise ValueError("symlinks in registry source tree are not supported")
    expected = package.get("checksum")
    if not isinstance(expected, str) or len(expected) != 64:
        raise ValueError("registry package has no locked SHA256 checksum")
    checksum_file = source / ".cargo-checksum.json"
    if checksum_file.exists():
        checksums = json.loads(checksum_file.read_text())
        if checksums.get("package") != expected:
            raise ValueError("registry package checksum differs from root Cargo.lock")
        files = checksums["files"]
        if not files:
            raise ValueError("empty registry source checksum map")
    else:
        # Cargo registry extraction normally has .cargo-ok, not a vendor-style
        # .cargo-checksum.json. Authenticate the cached archive against Cargo.lock
        # and derive its complete file checksum map without changing the registry.
        archive = source.parents[2] / "cache" / source.parent.name / f"{source.name}.crate"
        if hashlib.sha256(archive.read_bytes()).hexdigest() != expected:
            raise ValueError("cached crate archive checksum differs from root Cargo.lock")
        files = {}
        with tarfile.open(archive, "r:gz") as crate:
            for member in crate.getmembers():
                if member.isdir():
                    continue
                parts = Path(member.name).parts
                if not member.isfile() or not parts or parts[0] != source.name or ".." in parts:
                    raise ValueError("unexpected archive entry")
                entry = crate.extractfile(member)
                if entry is None:
                    raise ValueError("missing archive entry")
                files[str(Path(*parts[1:]))] = hashlib.sha256(entry.read()).hexdigest()
    for relative, checksum in files.items():
        path = Path(relative)
        if path.is_absolute() or ".." in path.parts:
            raise ValueError("unsafe registry checksum path")
        actual = source / path
        if actual.is_symlink() or not actual.is_file() or hashlib.sha256(actual.read_bytes()).hexdigest() != checksum:
            raise ValueError(f"registry source integrity failure: {relative}")
    actual_files = {str(path.relative_to(source)) for path in source.rglob("*") if path.is_file()}
    if actual_files - set(files) - {".cargo-ok", ".cargo-checksum.json"}:
        raise ValueError("unexpected files in registry source tree")


def run(args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def dependency(catalog, name, features=(), defaults=None):
    value = catalog[name]
    entry = {"version": value} if isinstance(value, str) else dict(value)
    entry.pop("path", None)
    if defaults is not None:
        entry["default-features"] = defaults
    entry["features"] = sorted(set(entry.get("features", []) + list(features)))
    return "{ " + ", ".join(f"{key} = {json.dumps(value)}" for key, value in entry.items()) + " }"


def prepare_sources(build, metadata, patch_directory, locked_packages, marker_value=MARKER_VALUE, expected_patches=None):
    """Use promoted repository-owned sources; never regenerate target patches."""
    owned_directory(build, marker_value)
    patch_crates = selected_patch_crates(patch_directory)
    if expected_patches is not None and patch_crates != expected_patches:
        raise ValueError("patch set changed during preparation")
    for patch in patch_crates:
        if (patch_directory / patch).read_bytes() != (ROOT / "vendor/patches" / patch).read_bytes():
            raise ValueError("historical experiment patch differs from promoted vendor input")
    run([sys.executable, "-B", str(ROOT / "vendor/verify.py")])
    for name, version in PATCH_VERSIONS.items():
        expected = ROOT / "vendor" / name / "Cargo.toml"
        matches = [p for p in metadata["packages"] if p["name"] == name and p["version"] == version
                   and Path(p["manifest_path"]).resolve() == expected.resolve()]
        if len(matches) != 1:
            raise ValueError(f"root metadata must use promoted vendor source: {name}")
    return dict(PATCH_VERSIONS)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--update-lock", action="store_true", help="explicitly refresh the checked-in experimental lockfile")
    parser.add_argument("--self-test", action="store_true", help="run preparation safety policy tests without touching the build")
    args = parser.parse_args()
    if args.self_test:
        result = unittest.main(argv=[sys.argv[0]], exit=False).result
        if not result.wasSuccessful():
            raise SystemExit(1)
        return
    if sys.platform != "linux":
        parser.error("this proof supports Linux Vulkan only")
    catalog = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["dependencies"]
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1"], cwd=ROOT))
    patch_versions = prepare_sources(BUILD, metadata, SOURCE / "patches",
                    tomllib.loads((ROOT / "Cargo.lock").read_text())["package"])
    deps = {
        "gpui": dependency(catalog, "gpui", ["x11", "wayland", "gpu-video-experiment"], False),
        "gpui_platform": dependency(catalog, "gpui_platform", ["x11", "wayland"]),
        "gpui_wgpu": dependency(catalog, "gpui_wgpu", ["gpu-video-experiment"]),
        "wgpu": dependency(catalog, "wgpu", ["vulkan"], False),
        "wgpu-hal": dependency(catalog, "wgpu-hal", ["vulkan"], False),
    }
    for name in ["ash", "glow", "libloading", "thiserror", "log"]:
        deps[name] = dependency(catalog, name)
    manifest = '\n'.join([
        '[workspace]', 'resolver = "3"', '', '[package]',
        'name = "gpui-mpv-gpu-experiment"', 'version = "0.0.0"',
        'edition = "2024"', 'publish = false', '', '[[bin]]',
        'name = "gpui-mpv-gpu-experiment"', f'path = {json.dumps(str(SOURCE / "src/main.rs"))}',
        '', '[dependencies]', *[f'{name} = {value}' for name, value in deps.items()],
        '', '[patch.crates-io]',
        *[f'{name} = {{ path = {json.dumps(str(ROOT / "vendor" / name))} }}' for name in patch_versions],
        '', '[profile.dev]', 'debug = 1', '', '[profile.dev.package."*"]', 'opt-level = 2', '',
    ])
    write_generated(BUILD / "Cargo.toml", manifest)
    write_generated(BUILD / "rust-toolchain.toml", (ROOT / "rust-toolchain.toml").read_text())
    lock = SOURCE / "Cargo.lock"
    generated_lock = BUILD / "Cargo.lock"
    if lock.is_symlink():
        raise ValueError("experimental checked-in lockfile cannot be a symlink")
    if generated_lock.is_symlink() or (generated_lock.exists() and not generated_lock.is_file()):
        raise ValueError("refusing nonregular generated lockfile")
    if args.update_lock:
        if lock.exists():
            write_generated(generated_lock, lock.read_text())
        run(["cargo", "update", "--workspace", "--offline"], cwd=BUILD)
        write_generated(lock, generated_lock.read_text())
    elif lock.exists():
        write_generated(generated_lock, lock.read_text())
    else:
        raise SystemExit("missing experiment Cargo.lock; run prepare.py --update-lock explicitly")
    run(["cargo", "metadata", "--locked", "--offline", "--no-deps", "--format-version", "1"], cwd=BUILD, stdout=subprocess.DEVNULL)
    print("Prepared isolated Linux GPU experiment (root lockfile and registry unchanged).")


class PreparationTests(unittest.TestCase):
    def test_patch_list_is_exact(self):
        patches = [Path(name) for name in PATCH_CRATES]
        self.assertEqual(validate_patches(patches), PATCH_CRATES)
        with self.assertRaises(ValueError):
            validate_patches(patches[:-1])
        with self.assertRaises(ValueError):
            validate_patches(patches + [Path("0004-unknown.patch")])

    def test_rejects_unknown_directory_and_preserves_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "existing"
            path.mkdir()
            (path / "user-file").write_text("keep")
            with self.assertRaises(ValueError):
                owned_directory(path)
            self.assertEqual((path / "user-file").read_text(), "keep")

    def test_owned_directory_and_symlink_refusal(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "owned"
            owned_directory(path)
            owned_directory(path)
            link = Path(temporary) / "link"
            link.symlink_to(path, target_is_directory=True)
            with self.assertRaises(ValueError):
                owned_directory(link)

    def test_generated_file_symlinks_never_overwrite_the_target(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / "user-file"
            source.write_text("keep")
            link = Path(temporary) / "Cargo.toml"
            link.symlink_to(source)
            with self.assertRaises(ValueError):
                write_generated(link, "replace")
            self.assertEqual(source.read_text(), "keep")

    def test_vendor_checksum_rejects_mutation_and_package_mismatch(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary)
            (source / "source.rs").write_text("original")
            checksum = "a" * 64
            (source / ".cargo-checksum.json").write_text(json.dumps({
                "package": checksum,
                "files": {"source.rs": hashlib.sha256(b"original").hexdigest()},
            }))
            verify_source(source, {"checksum": checksum})
            with self.assertRaises(ValueError):
                verify_source(source, {"checksum": "b" * 64})
            (source / "source.rs").write_text("mutated")
            with self.assertRaises(ValueError):
                verify_source(source, {"checksum": checksum})

    def test_verified_archive_fallback(self):
        with tempfile.TemporaryDirectory() as temporary:
            registry = Path(temporary)
            source = registry / "src/index/example-1.0"
            source.mkdir(parents=True)
            (source / "source.rs").write_text("original")
            archive = registry / "cache/index/example-1.0.crate"
            archive.parent.mkdir(parents=True)
            with tarfile.open(archive, "w:gz") as crate:
                crate.add(source / "source.rs", arcname="example-1.0/source.rs")
            package = {"checksum": hashlib.sha256(archive.read_bytes()).hexdigest()}
            verify_source(source, package)
            (source / "source.rs").write_text("mutated")
            with self.assertRaises(ValueError):
                verify_source(source, package)
            with self.assertRaises(ValueError):
                verify_source(source, {"checksum": "0" * 64})


if __name__ == "__main__":
    main()
