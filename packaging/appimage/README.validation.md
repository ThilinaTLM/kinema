# Validation boundaries

The automated gate targets x86_64 desktop hosts starting at Ubuntu 22.04.

| Check | Automated coverage | Remaining manual coverage |
| --- | --- | --- |
| Format | Type 2 header, AppDir entrypoint/metadata/icons and contained symlinks | External catalog extraction/integration |
| ABI | Version requirements of every shipped ELF, baseline C++ provider definitions | Unusual non-glibc hosts are unsupported |
| Dependency closure | Clean-host loader resolution, required QML and Qt/KDE/PipeWire modules | Runtime-only optional integrations |
| GUI | Actual final AppImage and portable archive, X11/Xvfb, software OpenGL, visible window and survival | Wayland and real graphics drivers |
| Media | Bundled libmpv initialization and local PCM decoding with null output | Embedded Qt render API, video codecs, hardware decode and physical audio |
| Runtime | FUSE-free launch in the distro matrix; normal FUSE launch on a runner | Host-specific FUSE/desktop integration policies |

Do not interpret a successful timeout, `--version`, SDK-host launch, or empty
`ldd` output as proof of GUI/playback compatibility. Release publication depends
on all automated jobs, but Wayland/embedded playback must also be checked before
tagging. AppImage catalog acceptance can only be asserted after its own retest
of the published release.

The 0.5.0 AppStream entry on main is a **development** entry. `scripts/release.sh
0.5.0` finalizes its type/date and offers the notes for review; it does not waive
the compatibility workflow. No tests require downloading or running the old
0.3.0 release.
