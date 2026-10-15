#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
import importlib.util
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET

RECIPE = Path(__file__).resolve().parents[1] / 'packaging/appimage'


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, RECIPE / filename)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


audit = module('audit', 'validate-appimage.py')
fetch = module('fetch', 'fetch-source.py')


class PackagingTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='kinema test ')
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.providers = set(json.loads((RECIPE / 'baseline-cxx.json').read_text()))

    def appdir(self):
        for name in ('AppRun', 'usr/bin/kinema'):
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('#!/bin/sh\nexit 0\n')
            path.chmod(0o755)
        desktop = self.root / 'usr/share/applications' / f'{audit.ID}.desktop'
        desktop.parent.mkdir(parents=True)
        desktop.write_text(f'[Desktop Entry]\nType=Application\nName=Kinema\nIcon={audit.ID}\nExec=kinema %U\n')
        (self.root / f'{audit.ID}.desktop').symlink_to(desktop.relative_to(self.root))
        (self.root / f'{audit.ID}.svg').write_text('<svg/>')
        (self.root / '.DirIcon').symlink_to(f'{audit.ID}.svg')
        metadata = self.root / f'usr/share/metainfo/{audit.ID}.metainfo.xml'
        metadata.parent.mkdir(parents=True)
        metadata.write_text(f'<component><id>{audit.ID}</id><launchable>{audit.ID}.desktop</launchable></component>')

    def test_dependency_lock_and_docker_groups(self):
        entries = json.loads((RECIPE / 'dependencies.lock.json').read_text())
        names = [entry['name'] for entry in entries]
        self.assertEqual(len(names), len(set(names)))
        for entry in entries + [s for e in entries for s in e.get('sources', [])]:
            self.assertRegex(entry['sha256'], r'^[0-9a-f]{64}$')
            self.assertTrue(entry['url'].startswith('https://'))
        groups = re.findall(r'^RUN bash /opt/recipe/build-dependencies.sh (.+)$',
                            (RECIPE / 'Dockerfile').read_text(), re.M)
        self.assertEqual([name for group in groups for name in group.split()], names)
        for tool in json.loads((RECIPE / 'tools.lock.json').read_text()):
            self.assertNotEqual(tool['version'], 'continuous')
            self.assertRegex(tool['sha256'], r'^[0-9a-f]{64}$')

    def test_upcoming_release_metadata_matches_project(self):
        root = RECIPE.parents[1]
        version = re.search(r'^    VERSION ([0-9.]+)$', (root / 'CMakeLists.txt').read_text(), re.M).group(1)
        metadata = ET.parse(root / 'data/dev.tlmtech.kinema.metainfo.xml').getroot()
        self.assertEqual(metadata.find('releases/release').get('version'), version)

    def test_release_notes_finalizer_preserves_history(self):
        # Execute only the metadata-editing Python heredoc, never the release
        # shell script (which tags/pushes and is intentionally maintainer-only).
        script = (RECIPE.parents[1] / 'scripts/release.sh').read_text()
        code = script.split('python3 - "$VERSION" "$TODAY" "$METAINFO" <<\'EOF\'\n', 1)[1].split('\nEOF', 1)[0]
        metadata = self.root / 'notes.xml'
        original = '''<component><releases>
    <release version="0.5.0" type="development" date="2026-10-02"><description><p>Keep notes</p></description></release>
    <release version="0.3.0" date="2026-08-22"/>
</releases></component>'''
        metadata.write_text(original)
        subprocess.run(['python3', '-c', code, '0.5.0', '2026-11-01', str(metadata)], check=True, capture_output=True)
        entries = ET.parse(metadata).findall('releases/release')
        self.assertEqual(len(entries), 2)
        self.assertEqual(entries[0].get('type'), 'stable')
        self.assertEqual(entries[0].get('date'), '2026-11-01')
        self.assertEqual(entries[0].findtext('description/p'), 'Keep notes')
        self.assertEqual(entries[1].get('date'), '2026-08-22')

    def test_rejects_mismatched_release_version_before_build(self):
        env = dict(os.environ, VERSION='0.0.0-invalid')
        result = subprocess.run(['bash', str(RECIPE / 'build-appimage.sh')],
                                env=env, text=True, capture_output=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn('does not match project', result.stderr)

    def test_numeric_versions(self):
        self.assertLess(audit.version_tuple('2.9'), audit.version_tuple('2.35'))
        self.assertFalse(audit.abi_errors({'GLIBC_2.9', 'GLIBC_2.35', 'CXXABI_1.3.13'}, self.providers))
        for version in ('GLIBC_2.38', 'GLIBC_2.100', 'GLIBC_PRIVATE', 'GLIBCXX_3.4.32', 'CXXABI_1.3.15'):
            with self.subTest(version=version):
                self.assertTrue(audit.abi_errors({version}, self.providers))

    def test_version_sections(self):
        text = '''Version symbols section '.gnu.version' contains 2 entries:
 Name: IGNORED
Version definition section '.gnu.version_d' contains 1 entry:
  Name: GLIBCXX_3.4.32
Version needs section '.gnu.version_r' contains 1 entry:
  Name: GLIBC_2.35 Flags: none Version: 2
'''
        self.assertEqual(audit.version_names(text, 'needs'), {'GLIBC_2.35'})
        self.assertEqual(audit.version_names(text, 'definition'), {'GLIBCXX_3.4.32'})

    def test_relative_metadata_symlinks_are_valid(self):
        self.appdir()
        audit.check_structure(self.root)

    def test_escaping_symlink(self):
        self.appdir()
        (self.root / 'escape').symlink_to('/bin/sh')
        with self.assertRaisesRegex(RuntimeError, 'Escaping symlink'):
            audit.check_structure(self.root)

    def test_missing_soname_link(self):
        self.appdir()
        (self.root / 'libpipewire-0.3.so.0').symlink_to('missing.so')
        with self.assertRaisesRegex(RuntimeError, 'Broken symlink'):
            audit.check_structure(self.root)

    def test_missing_diricon(self):
        self.appdir()
        (self.root / '.DirIcon').unlink()
        with self.assertRaisesRegex(RuntimeError, 'DirIcon'):
            audit.check_structure(self.root)

    def test_duplicate_desktop(self):
        self.appdir()
        (self.root / 'other.desktop').write_text('')
        with self.assertRaisesRegex(RuntimeError, 'Exactly one'):
            audit.check_structure(self.root)

    def test_wrong_metadata_id(self):
        self.appdir()
        path = self.root / f'usr/share/metainfo/{audit.ID}.metainfo.xml'
        path.write_text('<component><id>wrong</id></component>')
        with self.assertRaisesRegex(RuntimeError, 'ID mismatch'):
            audit.check_structure(self.root)

    def test_missing_plugins(self):
        with self.assertRaisesRegex(RuntimeError, 'Missing required runtime'):
            audit.check_plugins(self.root)

    def test_checksum_enforced_for_cached_and_downloaded_files(self):
        source = self.root / 'archive'
        source.write_bytes(b'known source')
        entry = dict(name='test', version='1', url=source.as_uri(), sha256=fetch.digest(source))
        cache = self.root / 'cache'
        result = fetch.fetch(entry, cache)
        self.assertEqual(result.read_bytes(), b'known source')
        result.write_bytes(b'corrupt')
        with self.assertRaisesRegex(RuntimeError, 'cached'):
            fetch.fetch(entry, cache)
        result.unlink()
        source.write_bytes(b'changed upstream')
        with self.assertRaisesRegex(RuntimeError, 'downloading'):
            fetch.fetch(entry, cache)
        self.assertFalse(result.exists())
        self.assertFalse(list(cache.glob('*.part')))

    def test_real_elf_need_section(self):
        self.assertIsNotNone(shutil.which('readelf'), 'binutils is required for packaging tests')
        names = audit.version_names(audit.run('readelf', '-W', '--version-info', '/bin/sh'), 'needs')
        self.assertTrue(any(n.startswith('GLIBC_') for n in names))

    def test_ldd_paths_containing_spaces(self):
        text = '  libfoo.so.1 => /tmp/relocated payload/usr/lib/libfoo.so.1 (0x00007fffff)\n'
        self.assertEqual(audit.resolutions(text), [('libfoo.so.1', '/tmp/relocated payload/usr/lib/libfoo.so.1')])

    def test_real_missing_dependency_and_absolute_rpath(self):
        self.assertIsNotNone(shutil.which('cc'), 'A C compiler is required for ELF fixtures')
        source = self.root / 'fixture.c'
        source.write_text('int dependency(void) { return 0; }\n')
        library = self.root / 'libfixture.so'
        subprocess.run(['cc', '-shared', '-fPIC', str(source), '-o', str(library)], check=True)
        source.write_text('int dependency(void); int main(void) { return dependency(); }\n')
        binary = self.root / 'fixture'
        subprocess.run(['cc', str(source), '-L' + str(self.root), '-lfixture', '-Wl,-rpath,$ORIGIN',
                        '-o', str(binary)], check=True)
        audit.check_elf(binary, self.root, self.providers, True)
        library.unlink()
        with self.assertRaisesRegex(RuntimeError, 'not found'):
            audit.check_elf(binary, self.root, self.providers, True)
        # A linked binary with no dependencies on the fixture still must not
        # carry its SDK's absolute search path into the AppImage.
        source.write_text('int main(void) { return 0; }\n')
        subprocess.run(['cc', str(source), '-Wl,-rpath,/opt/kinema-sdk/lib', '-o', str(binary)], check=True)
        with self.assertRaisesRegex(RuntimeError, 'non-relocatable'):
            audit.check_elf(binary, self.root, self.providers, False)

    def test_launcher_relocation_and_arguments(self):
        app = self.root / 'relocated application'
        (app / 'usr/bin').mkdir(parents=True)
        shutil.copy(RECIPE / 'AppRun.sh', app / 'AppRun')
        (app / 'AppRun').chmod(0o755)
        binary = app / 'usr/bin/kinema'
        binary.write_text('#!/bin/sh\nprintf "%s\\n" "$APPDIR" "$QML_IMPORT_PATH" "$QML2_IMPORT_PATH" "$@"\n')
        binary.chmod(0o755)
        output = subprocess.check_output([str(app / 'AppRun'), 'two words', '--help'], cwd='/', text=True).splitlines()
        self.assertEqual(output[0], str(app))
        self.assertEqual(output[1], output[2])
        self.assertEqual(output[-2:], ['two words', '--help'])


if __name__ == '__main__':
    unittest.main()
