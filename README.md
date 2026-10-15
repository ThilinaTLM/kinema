# Kinema Next

A pre-1.0 Rust desktop application using [GPUI Kit](https://gpui-kit.com), targeting
Windows, macOS, and Linux without KDE dependencies. **The first bounded catalog
increment is implemented in source; this is not the complete MVP.**

The planned MVP combines a local-first movie/series catalog, Torrentio/Peerflix
source discovery, debrid acquisition, managed local media storage, and a local
server for external VLC/mpv. Personal tracking, embedded playback, and a torrent
engine are deferred. See the [design docs](docs/README.md) for scope and open decisions.

## Current increment

Movies and Series offer explicit Cinemeta title search, backed by SQLite saved
membership for each exact query—not offline text search over all cached titles,
discovery/browsing, or detail/episode sync. Search reuses saved results for that
query and fetches on a miss; Refresh
requests Cinemeta again. Failed refreshes preserve previous results. There is no
automatic TTL refresh, global metadata-provider preference, or fallback yet.

The UI shows title names, IMDb IDs, optional overviews, loading/errors, and a last
successful fetch timestamp. This is a source-level status, not a claim of verified
GUI runtime or comprehensive live-provider support. Sources, debrid, managed media,
the local media server, and external playback remain unimplemented.
See [architecture](docs/architecture.md) for boundaries and lifecycle limits.

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
`kinema-desktop` (GPUI Kit UI and background catalog adapters). See [development](docs/development.md) for
core-only checks and OS setup. Release packaging is not yet provided.

## C++ reference

The previous Qt/KDE application is preserved under [reference/cpp](reference/cpp/README.md).
It is historical reference, not a compatibility layer or a promise of feature parity.
Next starts fresh; old settings, databases, and keyring entries will not be migrated.
Historical release binaries do not represent this Rust scaffold.

## License

Original Kinema code remains [Apache-2.0](LICENSE). See [NOTICE](NOTICE) for
third-party notices and the distinction between the scaffold and archived dependencies.
