#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Download one locked archive, verifying cached files too. Prints only its path."""
import hashlib
import json
import os
from pathlib import Path
import sys
import urllib.request


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def fetch(entry, cache):
    cache.mkdir(parents=True, exist_ok=True)
    path = cache / (entry['name'] + '-' + entry['version'] + '.tar')
    if path.exists():
        if digest(path) != entry['sha256']:
            raise RuntimeError(f'Checksum mismatch in cached {path}; remove it and retry')
        return path
    temporary = path.with_suffix('.part')
    try:
        request = urllib.request.Request(entry['url'], headers={'User-Agent': 'Kinema packaging'})
        with urllib.request.urlopen(request, timeout=120) as src, temporary.open('wb') as dst:
            for chunk in iter(lambda: src.read(1024 * 1024), b''):
                dst.write(chunk)
        if digest(temporary) != entry['sha256']:
            raise RuntimeError(f'Checksum mismatch downloading {entry["name"]}')
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)
    return path


if __name__ == '__main__':
    entries = json.loads(Path(sys.argv[1]).read_text())
    all_entries = entries + [source for entry in entries for source in entry.get('sources', [])]
    entry = next(e for e in all_entries if e['name'] == sys.argv[2])
    print(fetch(entry, Path(sys.argv[3]).resolve()))
