#!/usr/bin/env python3
"""Authenticate/reproduce repository patches offline; never modify registry sources.

Default: verify checked-in source digests. --reproduce OUTPUT: recreate in a NEW
output directory from cached crates.io archives and compare with those digests.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parents[1]
VENDOR = ROOT / "vendor"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inventory(directory, names):
    result = {}
    for name in names:
        source = directory / name
        if source.is_symlink() or not source.is_dir():
            raise ValueError(f"missing/nonregular vendor directory: {name}")
        for path in sorted(source.rglob("*")):
            if path.is_symlink():
                raise ValueError(f"symlink in vendor sources: {path}")
            if path.is_file():
                result[path.relative_to(directory).as_posix()] = digest(path)
    return result


def reproduce(output, provenance):
    # Refuse even empty existing destinations; no unknown files are replaced.
    output.mkdir(parents=True, exist_ok=False)
    cache = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")) / "registry/cache"
    for package in provenance["packages"]:
        name, version = package["name"], package["version"]
        archive_name = f"{name}-{version}"
        archives = sorted(cache.glob(f"*/{archive_name}.crate"))
        archives = [path for path in archives if digest(path) == package["checksum"]]
        if not archives:
            raise ValueError(f"no authenticated cached archive: {archive_name}")
        destination = output / name
        destination.mkdir()
        with tarfile.open(archives[0], "r:gz") as archive:
            for member in archive.getmembers():
                parts = Path(member.name).parts
                if not parts or parts[0] != archive_name or ".." in parts or Path(member.name).is_absolute():
                    raise ValueError("unsafe archive entry")
                if member.isdir():
                    continue
                if not member.isfile() or len(parts) < 2:
                    raise ValueError("nonregular archive entry")
                path = destination.joinpath(*parts[1:])
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(archive.extractfile(member).read())
    for patch in sorted(provenance["patches"]):
        # Numbered patch names include their exact crate release.
        release = patch.split("-", 1)[1].removesuffix(".patch")
        package = next(p for p in provenance["packages"] if release == f'{p["name"]}-{p["version"]}')
        subprocess.run(["patch", "--batch", "--forward", "--fuzz=0", "-p1"],
                       input=(VENDOR / "patches" / patch).read_bytes(),
                       cwd=output / package["name"], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reproduce", type=Path, metavar="NEW_DIRECTORY")
    args = parser.parse_args()
    provenance = json.loads((VENDOR / "provenance.json").read_text())
    for name, expected in provenance["patches"].items():
        if digest(VENDOR / "patches" / name) != expected:
            raise ValueError(f"patch checksum mismatch: {name}")
    directory = VENDOR
    if args.reproduce:
        directory = args.reproduce.resolve()
        reproduce(directory, provenance)
    actual = inventory(directory, [p["name"] for p in provenance["packages"]])
    expected = json.loads((VENDOR / "sources.sha256.json").read_text())
    if actual != expected:
        raise ValueError("vendor source inventory differs from checked-in SHA-256 manifest")
    print(f"Verified {len(actual)} patched source files.")


if __name__ == "__main__":
    main()
