# Kinema

A pre-1.0 Rust desktop application using direct GPUI (`gpui-pre =0.3.9`) and
`gpui_platform`, targeting Windows, macOS, and Linux. The Rust workspace is the
only active implementation; it has no Qt/KDE application dependencies.
V3 is the sole/default UI, with in-house widgets and no UI version switch. **The first bounded catalog
increment has expanded into integrated MVP source flows; full MVP validation
is not complete.**

The planned MVP combines a local-first movie/series catalog, Torrentio/Peerflix
source discovery, debrid acquisition, managed local media storage, and a local
server for default GPU-surface Embedded MPV or explicit external VLC/mpv.
Personal tracking and a torrent engine are outside this playback increment. See the [design docs](docs/README.md) for scope and open decisions.

## Current increment

Source wiring now covers Cinemeta/TMDB browse, search, details and episodes;
Torrentio/Peerflix sources; Real-Debrid/AllDebrid adapters; settings with secure
credentials; managed media and loopback serving to Embedded MPV or external players.
Settings control metadata provider priority and background enrichment; cached results
do not refresh automatically by TTL. TMDB requires a user bearer token, and titles
need IMDb mapping before source lookup.

Start with the [quickstart and setup](docs/development.md#quickstart-and-setup).
Local-server/mpv smoke evidence does **not** establish live paid-provider playback,
complete GUI acceptance or cross-platform runtime support. Automated workspace
checks are separate from provider/UI acceptance and native runtime qualification.
See the [validation matrix](docs/development.md#validation-status-and-limits) and
[architecture](docs/architecture.md) for current limits. No full-MVP completion
claim is made.

## Development

Install Rust **1.97.1** through rustup and the
[platform prerequisites](docs/development.md). From the repository root:

```sh
cargo run -p kinema-desktop --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

The default Cargo feature `gpu-surface` enables the sole/default Embedded MPV
renderer using durable vendored GPUI/HAL patches in the root workspace; no
experimental wrapper is required. Supply native [libmpv and platform prerequisites](docs/development.md#embedded-mpv-prerequisites-and-validation-plan)
separately. External mpv/VLC are unchanged. Prior scoped Linux evidence is not
new default-build qualification; native Windows/macOS tests and HDR remain
unqualified. Default-on does not imply release readiness.

With `debug_assertions` enabled (default debug builds), catalog data lives in
`./data/` relative to the process working directory; the repository-root directory
is gitignored. `cargo run -p kinema-desktop --release --locked` disables debug
assertions by default and uses native app data resolved through
`ProjectDirs::from("", "", "kinema").data_local_dir()` instead. Switching modes
uses separate databases; existing data is not migrated, moved, or deleted.
See [storage locations](docs/storage-layout.md#implemented-catalog-location).

The workspace contains `kinema-core` (UI-independent domain code),
`kinema-desktop` (direct-GPUI v3 UI and background catalog adapters), and
`kinema-mpv` (native embedded playback). See the [feature overview](docs/features.md)
and [development guide](docs/development.md) for current scope, core-only checks,
and OS setup. Release packaging is not yet provided.

## Historical archive

The previous Qt/KDE application is preserved under [reference/cpp](reference/cpp/README.md).
It is historical reference, not a compatibility layer or a promise of feature parity.
The Rust application starts fresh; old settings, databases, and keyring entries
will not be migrated.
Historical release binaries do not represent this Rust scaffold.

## License

Original Kinema code remains [Apache-2.0](LICENSE). See [NOTICE](NOTICE) for
third-party notices and the distinction between the scaffold and archived dependencies.
