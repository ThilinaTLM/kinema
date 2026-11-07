# Desktop releases and distribution

**Status:** Draft · **Area:** platform

## Problem

There is no way to install Kinema except building it from source. Embedded
playback also depends on native libraries that users must find themselves.

## Current state

- `packaging/` has Linux `.desktop` and AppStream files and the generated icons
  (see [development](../architecture/development.md#packaging)).
- libmpv is loaded dynamically and not bundled. Windows also needs ANGLE DLLs
  next to the executable.
- There is no release CI.

## Recommendation

1. **A release pipeline:** a tag triggers builds of the artifacts and a GitHub
   release.
2. **Windows:** an installer (MSI or NSIS) with an embedded EXE icon. Decide how
   libmpv and ANGLE get there: bundle them, or use a pinned, checksummed fetch
   that the user confirms.
3. **macOS:** an app bundle with Info.plist and ICNS, signing and
   notarization. Bundle libmpv inside the app or depend on Homebrew.
4. **Linux:** AppImage and/or Flatpak, plus a standalone tarball. Add release
   history and screenshots to AppStream.
5. **Licensing review:** bundling libmpv or FFmpeg brings GPL/LGPL obligations,
   and patent-encumbered codecs carry their own. This review must be finished
   before anything is bundled.

## Open questions

- Which minimum OS versions and architectures should be supported?
- Is bundling libmpv acceptable under its license, or should users install it
  themselves?
- Should there be update notifications or auto-update?
