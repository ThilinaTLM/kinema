# Kinema Next

A Rust desktop scaffold using [GPUI Kit](https://gpui-kit.com), targeting
Windows, macOS, and Linux without KDE dependencies. **This is not a complete
application or MVP.**

The planned MVP combines a local-first movie/series catalog, Torrentio/Peerflix
source discovery, debrid acquisition, managed local media storage, and a local
server for external VLC/mpv. Personal tracking, embedded playback, and a torrent
engine are deferred. See the [design docs](docs/README.md) for scope and open decisions.

## Development

Install Rust **1.97.1** through rustup and the
[platform prerequisites](docs/development.md). From the repository root:

```sh
cargo run -p kinema-desktop --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

The workspace contains `kinema-core` (UI-independent domain code) and
`kinema-desktop` (GPUI Kit shell). See [development](docs/development.md) for
core-only checks and OS setup. Release packaging is not yet provided.

## C++ reference

The previous Qt/KDE application is preserved under [reference/cpp](reference/cpp/README.md).
It is historical reference, not a compatibility layer or a promise of feature parity.
Next starts fresh; old settings, databases, and keyring entries will not be migrated.
Historical release binaries do not represent this Rust scaffold.

## License

Original Kinema code remains [Apache-2.0](LICENSE). See [NOTICE](NOTICE) for
third-party notices and the distinction between the scaffold and archived dependencies.
