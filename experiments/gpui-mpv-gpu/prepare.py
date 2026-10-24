#!/usr/bin/env python3
"""Materialize the isolated experiment; never mutate Cargo registry sources."""
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
}


def validate_patches(patches):
    if {path.name for path in patches} != set(PATCH_CRATES):
        raise ValueError("expected exactly the three checked-in GPUI/WGPU/HAL experiment patches")


def owned_directory(path):
    if path.is_symlink():
        raise ValueError(f"refusing symlink directory: {path.name}")
    if path.exists():
        marker = path / MARKER
        if not marker.is_file() or marker.is_symlink() or marker.read_text() != MARKER_VALUE:
            raise ValueError(f"refusing non-owned preexisting directory: {path.name}")
    else:
        path.mkdir(parents=True)
        (path / MARKER).write_text(MARKER_VALUE)


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
    owned_directory(BUILD)
    for directory in [BUILD / "patched", BUILD / "patched.new"]:
        if directory.exists() or directory.is_symlink():
            owned_directory(directory)
    locked_packages = tomllib.loads((ROOT / "Cargo.lock").read_text())["package"]
    patches = sorted((SOURCE / "patches").glob("*.patch"))
    validate_patches(patches)
    crates = {"gpui-pre": "0.3.9", "gpui-pre-wgpu": "0.3.9", "wgpu-hal": "29.0.4"}
    patch_sources = {}
    for name, version in crates.items():
        matches = [p for p in metadata["packages"] if p["name"] == name and p["version"] == version and (p.get("source") or "").startswith("registry+")]
        if len(matches) != 1:
            raise SystemExit(f"expected exactly one registry {name} ={version}")
        package = matches[0]
        source = Path(package["manifest_path"]).parent
        locked = [p for p in locked_packages if p["name"] == name
                  and p["version"] == version and p.get("source") == package["source"]]
        if len(locked) != 1:
            raise ValueError(f"missing exact registry package in root Cargo.lock: {name}")
        verify_source(source, locked[0])
        patch_sources[name] = source
    digest = hashlib.sha256()
    for name, path in patch_sources.items():
        digest.update(f"{name}:{path}".encode())
    for patch in patches:
        digest.update(patch.name.encode() + patch.read_bytes())
    stamp = BUILD / "sources.sha256"
    fingerprint = digest.hexdigest()
    if not stamp.exists() or stamp.read_text() != fingerprint:
        temporary = BUILD / "patched.new"
        if temporary.exists():
            owned_directory(temporary)
            shutil.rmtree(temporary)
        owned_directory(temporary)
        for name, source in patch_sources.items():
            shutil.copytree(source, temporary / name)
        # Each checked-in patch is rooted at its copied crate (a/src/...).
        for patch in patches:
            run(["patch", "--batch", "--forward", "-p1", "-i", str(patch)],
                cwd=temporary / PATCH_CRATES[patch.name])
        destination = BUILD / "patched"
        if destination.exists():
            owned_directory(destination)
            shutil.rmtree(destination)
        temporary.rename(destination)
        stamp.write_text(fingerprint)
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
        *[f'{name} = {{ path = "patched/{name}" }}' for name in crates],
        '', '[profile.dev]', 'debug = 1', '', '[profile.dev.package."*"]', 'opt-level = 2', '',
    ])
    (BUILD / "Cargo.toml").write_text(manifest)
    shutil.copy2(ROOT / "rust-toolchain.toml", BUILD / "rust-toolchain.toml")
    lock = SOURCE / "Cargo.lock"
    if args.update_lock:
        if lock.exists():
            shutil.copy2(lock, BUILD / "Cargo.lock")
        run(["cargo", "generate-lockfile", "--offline"], cwd=BUILD)
        shutil.copy2(BUILD / "Cargo.lock", lock)
    elif lock.exists():
        shutil.copy2(lock, BUILD / "Cargo.lock")
    else:
        raise SystemExit("missing experiment Cargo.lock; run prepare.py --update-lock explicitly")
    run(["cargo", "metadata", "--locked", "--offline", "--no-deps", "--format-version", "1"], cwd=BUILD, stdout=subprocess.DEVNULL)
    print("Prepared isolated Linux GPU experiment (root lockfile and registry unchanged).")


class PreparationTests(unittest.TestCase):
    def test_patch_list_is_exact(self):
        patches = [Path(name) for name in PATCH_CRATES]
        validate_patches(patches)
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
