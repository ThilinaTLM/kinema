# Kinema Next

A pre-1.0 Rust desktop application using direct GPUI (`gpui-pre =0.3.9`) and
`gpui_platform`, targeting Windows, macOS, and Linux without KDE dependencies.
V3 is the sole/default UI, with in-house widgets and no UI version switch. **The first bounded catalog
increment has expanded into integrated MVP source flows; full MVP validation
is not complete.**

The planned MVP combines a local-first movie/series catalog, Torrentio/Peerflix
source discovery, debrid acquisition, managed local media storage, and a local
server for external VLC/mpv. Personal tracking, embedded playback, and a torrent
engine are deferred. See the [design docs](docs/README.md) for scope and open decisions.

## Current increment

Source wiring now covers Cinemeta/TMDB browse, search, details and episodes;
Torrentio/Peerflix sources; Real-Debrid/AllDebrid adapters; settings with secure
credentials; managed media and loopback serving to external players. Both metadata
providers are explicit selections, with no automatic fallback or TTL refresh.
TMDB requires a user bearer token and IMDb mapping before source lookup.

Start with the [quickstart and setup](docs/development.md#quickstart-and-setup).
Local-server/mpv smoke evidence does **not** establish live paid-provider playback,
complete GUI acceptance or cross-platform runtime support. Historical locked root
checks predate the v3 cleanup; current validation and provider/UI acceptance gaps
are tracked separately.
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

With `debug_assertions` enabled (default debug builds), catalog data lives in
`./data/` relative to the process working directory; the repository-root directory
is gitignored. `cargo run -p kinema-desktop --release --locked` disables debug
assertions by default and uses native app data resolved through
`ProjectDirs::from("", "", "kinema").data_local_dir()` instead. Switching modes
uses separate databases; existing data is not migrated, moved, or deleted.
See [storage locations](docs/storage-layout.md#implemented-catalog-location).

The workspace contains `kinema-core` (UI-independent domain code) and
`kinema-desktop` (direct-GPUI v3 UI and background catalog adapters). See [development](docs/development.md) for
core-only checks and OS setup. Release packaging is not yet provided.

## C++ reference

The previous Qt/KDE application is preserved under [reference/cpp](reference/cpp/README.md).
It is historical reference, not a compatibility layer or a promise of feature parity.
Next starts fresh; old settings, databases, and keyring entries will not be migrated.
Historical release binaries do not represent this Rust scaffold.

## License

Original Kinema code remains [Apache-2.0](LICENSE). See [NOTICE](NOTICE) for
third-party notices and the distinction between the scaffold and archived dependencies.
