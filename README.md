# Kinema

[![Rust CI](https://github.com/ThilinaTLM/kinema/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/ThilinaTLM/kinema/actions/workflows/ci.yml)

A pre-1.0 Rust desktop app for movies and TV series, built directly on GPUI
(`gpui-pre =0.3.9`) and targeting Windows, macOS and Linux without KDE dependencies.

Kinema browses a local-first movie and series catalog (Cinemeta, TMDB), finds
sources (Torrentio, Peerflix), resolves them through debrid (Real-Debrid,
AllDebrid), downloads into managed storage and plays from a loopback server in an
embedded mpv window or external mpv/VLC. A personal Library tracks membership,
watched state and resume progress. Linux is the only hands-on tested platform;
Windows/macOS runtime behavior is unqualified.

## Documentation

- [Architecture](docs/architecture/README.md): principles, engineering decisions
  and the strict [design system](docs/architecture/design-system.md).
- [Proposals and roadmap](docs/proposals/README.md): drafts of new ideas and
  designs, plus the planned work.

## Development

Install Rust **1.97.1** through rustup, the
[platform prerequisites](docs/architecture/development.md#prerequisites) and a
native libmpv. From the repository root:

```sh
cargo run -p kinema-desktop --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Debug builds keep all data in `./data/` (relative to the working directory);
release builds use native app directories. See
[data locations](docs/architecture/data.md#locations).

## Data and compatibility

Kinema starts fresh: old `~/.config/kinemarc`, `kinema.db`, and keyring entries
are not migrated.

## License

Original Kinema code remains [Apache-2.0](LICENSE). See [NOTICE](NOTICE) for
third-party code, bundled-asset credits, and distribution obligations.
