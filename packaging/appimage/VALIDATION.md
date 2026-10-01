# 0.5.0 development bundle qualification

Validated on 2026-10-02 from the working tree, using the checked-in Ubuntu 22.04
Docker SDK recipe. No published 0.3.0 artifact was downloaded or tested.

- Complete source SDK build: passed (resumed after an executor interruption).
- Subsequent SDK build: all dependency layers cached; passed.
- Official SDK container -> final AppImage and portable archive: passed.
- Structural/AppStream/desktop/ABI audit: passed, 333 ELF objects, maximum
  required glibc version **2.35**.
- Clean Ubuntu 22.04, Ubuntu 24.04 and Debian 13: all ELF relocations resolved,
  actual AppImage/portable GUI windows appeared and remained running, CLI checks
  passed, and bundled libmpv decoded/advanced generated local PCM audio.
- FUSE launch: passed; executable filesystem verified as `fuse.Kinema.AppImage`.
- Native Wayland startup on Arch Linux: passed with disposable settings and a
  private D-Bus session. Qt created its OpenGL QRhi using the real Intel/Mesa GPU.
- Project build and all 105 CTest tests: passed. Packaging contracts: 18 passed.
- ShellCheck, shell syntax, actionlint, metadata validation and diff checks: passed.

Qualified development AppImage SHA-256:

```
3b3360a722f3b3235b21e74708eb719e8aa05f6d11ed042f84560980aeeda308
```

This identifies the local `Kinema-0.5.0-dev-x86_64.AppImage`, not the future release
artifact. The release workflow rebuilds and requalifies the tagged source; do
not publish the development image as the release or expect byte-identical hashes.

Remaining boundaries: physical audio output, embedded Qt video playback and
hardware decoding need a normal end-to-end desktop check; automated media testing
uses a null output. External AppImage catalog acceptance requires `/retest` on
PR 7587 after publishing the validated release. See `README.validation.md` and
`../README.md` for the release checklist and coverage limits.
