#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Audit our own trusted AppImage/AppDir. Requires binutils and metadata tools.

Run --dependencies on clean hosts, not on the SDK host (which masks omissions).
ldd/extraction execute artifact code: never use this on untrusted downloads.
"""
import argparse
import fnmatch
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import xml.etree.ElementTree as ET

ID = 'dev.tlmtech.kinema'
HERE = Path(__file__).resolve().parent


def run(*args, env=None):
    result = subprocess.run(args, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, env=env, check=False)
    if result.returncode:
        raise RuntimeError(f'{args}:\n{result.stdout}')
    return result.stdout


def version_names(text, section):
    """Read only .gnu.version_r (needs) or .gnu.version_d (definitions)."""
    active = False
    names = set()
    for line in text.splitlines():
        if line.startswith('Version '):
            active = line.startswith(f'Version {section} section')
        if active:
            names.update(re.findall(r'\bName: (\S+)', line))
    return names


def version_tuple(value):
    return tuple(int(n) for n in value.split('.'))


def abi_errors(needs, providers):
    errors = []
    for name in sorted(needs):
        if name.startswith('GLIBC_'):
            suffix = name.removeprefix('GLIBC_')
            if not re.fullmatch(r'\d+(\.\d+)*', suffix) or version_tuple(suffix) > (2, 35):
                errors.append(f'unsupported {name} (baseline GLIBC_2.35)')
        if name.startswith(('GLIBCXX_', 'CXXABI_')) and name not in providers:
            errors.append(f'{name} is not exported by the baseline C++ runtime')
    return errors


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def check_structure(root):
    root = root.resolve()
    for path in root.rglob('*'):
        if path.is_symlink():
            require(path.exists(), f'Broken symlink: {path}')
            require(path.resolve().is_relative_to(root), f'Escaping symlink: {path}')
    for name in ('AppRun', 'usr/bin/kinema'):
        require(os.access(root / name, os.X_OK), f'Missing executable {name}')
    desktops = list(root.glob('*.desktop'))
    require(len(desktops) == 1, 'Exactly one root desktop file is required')
    desktop = desktops[0]
    fields = dict(re.findall(r'^(\w+)=(.*)$', desktop.read_text(), re.M))
    require(fields.get('Type') == 'Application', 'Desktop Type must be Application')
    require(fields.get('Icon') == ID, 'Desktop icon ID mismatch')
    require(fields.get('Exec', '').split()[:1] == ['kinema'], 'Unexpected desktop Exec')
    require(any((root / f'{ID}.{ext}').is_file() for ext in ('svg', 'png')), 'Missing root icon')
    require((root / '.DirIcon').is_file(), 'Missing .DirIcon')
    metadata = root / f'usr/share/metainfo/{ID}.metainfo.xml'
    require(metadata.is_file(), 'Missing AppStream metadata')
    xml = ET.parse(metadata).getroot()
    require(xml.findtext('id') == ID, 'AppStream component ID mismatch')
    require(xml.findtext('launchable') == f'{ID}.desktop', 'AppStream desktop ID mismatch')
    return desktop, metadata


def check_plugins(root):
    files = [str(p.relative_to(root)) for p in root.rglob('*') if p.is_file()]
    for pattern in (
        '*/platforms/libqxcb.so', '*/platforms/libqwayland-generic.so',
        '*/sqldrivers/libqsqlite.so', '*/imageformats/libqsvg.so',
        '*/wayland-shell-integration/libxdg-shell.so',
        '*/wayland-graphics-integration-client/libqt-plugin-wayland-egl.so',
        '*/tls/libqopensslbackend.so', '*/kf6/kirigami/platform/*.so',
        '*/qml/QtQuick/qmldir', '*/qml/org/kde/kirigami/qmldir',
        '*/qml/org/kde/kirigamiaddons/formcard/qmldir',
        '*/qml/org/kde/kirigamiaddons/settings/qmldir',
        '*/qml/org/kde/desktop/qmldir', '*/libpipewire-0.3.so.0',
        '*/spa-0.2/support/libspa-support.so', '*/spa-0.2/audioconvert/libspa-audioconvert.so',
        '*/pipewire-0.3/libpipewire-module-client-node.so',
        '*/kf6/kio/kio_http.so', '*/kf6/kio/kio_file.so', 'usr/libexec/kf6/kioworker',
        'usr/share/pipewire/client.conf',
    ):
        require(any(fnmatch.fnmatch(f, pattern) for f in files), f'Missing required runtime: {pattern}')


def elf_files(root):
    for path in root.rglob('*'):
        if path.is_file() and not path.is_symlink():
            with path.open('rb') as stream:
                header = stream.read(20)
            if header[:4] == b'\x7fELF':
                require(header[4:6] == b'\x02\x01' and header[18:20] == b'\x3e\x00',
                        f'Not an x86_64 ELF: {path}')
                yield path


def resolutions(text):
    # ldd paths may contain spaces when testing a relocated AppDir.
    return re.findall(r'^\s*(\S+) => (/.+?)\s+\(0x[0-9a-f]+\)', text, re.M)


def check_elf(path, root, providers, dependencies):
    relative = path.relative_to(root)
    needs = version_names(run('readelf', '-W', '--version-info', str(path)), 'needs')
    errors = abi_errors(needs, providers)
    dynamic = run('readelf', '-W', '-d', str(path))
    forbidden = {'libc.so.6', 'libm.so.6', 'libpthread.so.0', 'libdl.so.2', 'ld-linux-x86-64.so.2',
                 'libstdc++.so.6', 'libgcc_s.so.1'}
    for soname in re.findall(r'\(SONAME\).*?\[(.*?)\]', dynamic):
        if soname in forbidden:
            errors.append(f'host-coupled runtime must not be bundled: {soname}')
    for value in re.findall(r'\((?:RPATH|RUNPATH)\).*?\[(.*?)\]', dynamic):
        for part in value.split(':'):
            if not re.match(r'^\$(?:ORIGIN|\{ORIGIN\})(?:/|$)', part):
                errors.append(f'non-relocatable runtime search path: {part!r}')
            else:
                expanded = part.replace('${ORIGIN}', str(path.parent)).replace('$ORIGIN', str(path.parent))
                if not Path(expanded).resolve().is_relative_to(root):
                    errors.append(f'runtime search path escapes AppDir: {part!r}')
    if dependencies and '(NEEDED)' in dynamic:
        env = dict(os.environ, LC_ALL='C', LD_LIBRARY_PATH=f'{root}/usr/lib:{root}/usr/lib/x86_64-linux-gnu')
        env.pop('LD_PRELOAD', None)
        output = run('ldd', '-r', str(path), env=env)
        # Versioned-symbol checks alone miss newer unversioned Wayland/X11
        # functions. Resolve relocations in every plugin, including unused ones.
        if 'not found' in output or 'undefined symbol:' in output:
            errors.append(output.strip())
        policy = json.loads((HERE / 'host-libraries.json').read_text())
        for soname, target in resolutions(output):
            if not Path(target).resolve().is_relative_to(root) and not any(fnmatch.fnmatch(Path(soname).name, p) for p in policy):
                errors.append(f'{soname} unexpectedly resolved from host: {target}')
    require(not errors, f'{relative}:\n  ' + '\n  '.join(errors))
    return needs


def validate(root, args):
    desktop, metadata = check_structure(root)
    run('desktop-file-validate', str(desktop))
    run('appstreamcli', 'validate', '--no-net', str(metadata))
    check_plugins(root)
    require((root / 'AppRun').read_bytes() == (HERE / 'AppRun.sh').read_bytes(),
            'Packed AppRun differs from the intended launcher')
    providers = set(json.loads(Path(args.baseline).read_text()))
    report = {}
    errors = []
    for path in elf_files(root):
        try:
            report[str(path.relative_to(root))] = sorted(check_elf(path, root, providers, args.dependencies))
        except RuntimeError as error:
            errors.append(str(error))
    require(report or errors, 'No ELF payload found')
    if args.report:
        Path(args.report).write_text(json.dumps({'requirements': report, 'errors': errors}, indent=2) + '\n')
    require(not errors, '\n'.join(errors))
    print(f'Validated {len(report)} ELF objects in {root}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('artifact', type=Path)
    parser.add_argument('--baseline', default=str(HERE / 'baseline-cxx.json'))
    parser.add_argument('--dependencies', action='store_true')
    parser.add_argument('--report')
    args = parser.parse_args()
    artifact = args.artifact.resolve()
    os.environ['LC_ALL'] = 'C'
    if artifact.is_dir():
        validate(artifact, args)
        return
    with artifact.open('rb') as stream:
        header = stream.read(11)
    require(header[:4] == b'\x7fELF' and header[8:11] == b'AI\x02', 'Not a Type 2 AppImage')
    with tempfile.TemporaryDirectory(prefix='kinema-appimage-') as temporary:
        subprocess.run([str(artifact), '--appimage-extract'], cwd=temporary,
                       stdout=subprocess.DEVNULL, check=True)
        validate(Path(temporary) / 'squashfs-root', args)


if __name__ == '__main__':
    main()
