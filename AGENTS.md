- Prefer clean designs over compatibility shims. Do not migrate old
  `~/.config/kinemarc`, `kinema.db`, or keyring entries.
- Use Rust 1.97.1, edition 2024, resolver 3. Keep shared dependency versions in the
  root workspace; commit intentional lockfile updates, and use `--locked` for checks.
- Keep domain types and pure policies in `kinema-core`; keep GPUI and platform
  presentation in `kinema-desktop`. Do not add unused framework layers or subsystems.
- Use direct `gpui` (package `gpui-pre =0.3.9`) and `gpui_platform`, with in-house
  components under `kinema-desktop/src/widgets` per `docs/design/desktop-v3.md`.
  V3 is the sole/default UI; do not restore legacy UI, version switches, or
  `gpui-kit`.
- Embedded MPV uses the sole/default GPU-surface path via default Cargo feature
  `gpu-surface`. Keep root `cargo run -p kinema-desktop --locked` working with
  durable vendored GPUI/HAL patches; do not require an experimental wrapper or
  edit registry sources. External mpv/VLC are unchanged. Default-on is not new
  qualification: native Windows/macOS tests and HDR remain unqualified.
- Target Windows, macOS, and Linux without KDE dependencies. Avoid platform path,
  shell, and file-lifetime assumptions; launch external players with argument lists.
- Keep blocking I/O, SQL, network waits, and decoding outside UI rendering. Own
  background tasks, cancel obsolete work, and reject stale results with generations.
- Keep credentials in OS secure storage, never ordinary settings, databases,
  manifests, or logs. Distinguish service failures from successful empty results.
- Add focused tests for domain rules and failure paths. Do not claim real provider,
  playback, or OS support based only on a compiling scaffold.
- Validate from the repository root:
  `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`, and
  `cargo test --workspace --locked`.
  See `docs/development.md` for prerequisites and core-only checks.
