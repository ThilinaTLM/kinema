# Backend source provenance

Imported on 2026-10-22 from the local `mpv-gpui` experiment at
`/home/tlm/Projects/Random/mpv-gpui`. That snapshot has no Git metadata;
these SHA-256 hashes identify the exact source bytes read at import time.
The experiment has not been modified. Its Apache-2.0 LICENSE is retained verbatim.
No separate NOTICE or named-author attribution file was present in the source root.

Imported **13 files verbatim initially**: 12 Rust files (`src/mpv/**`,
`src/state.rs`, and `src/error.rs`) plus `LICENSE`. The crate now contains 13
Rust files including its rewritten `src/lib.rs`; the manifest below additionally
records source Cargo.toml, README.md and lib.rs as extraction context.
Replaced `src/lib.rs`
with backend-only exports; intentionally excluded the GPUI view, examples,
GPUI 0.2.2 dependency and libc workaround. Cargo dependencies use the Kinema
workspace. Kinema changes add observed cache pause/media-stop state, terminal
worker/fatal-error reporting, unexpected-worker-exit guards, an opt-in consuming
RGBA-to-BGRA frame conversion, explicit script/OSC/OSD suppression options,
and focused tests.
All other backend implementation originates in this experiment snapshot.

## Source SHA-256 manifest

```text
674a3fe5f19c39fcacf573b7d0dcc8cfa69067f7f9cc4d99e0fe0aae001e98e9  Cargo.toml
cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30  LICENSE
6b6061beaecc854e90d88b59fe233aec89ad85d1aa474013cf4a67cffddb868a  README.md
bc3a0aba75356cfed9f96c6ab0299b5cc191a782e12bb2c3619685ac0e571785  src/lib.rs
f01a55ef5e10ba788d1b562b053b4483f6e3ac74bdf5a9befa9faefd08f2694c  src/state.rs
a95185c2844743c7b37944f1e64c28210b6c2c5a3179573a6bbb6722c4b9e0c1  src/error.rs
dbdab5f348e4dd50334e91ad5fae722ac15973e9b23a0a17297696c2188dfb5a  src/mpv/ffi.rs
4bcd38c3f0ddfe7da94d4072d72de1ecb41814e9d23310364a137a6d340bbf54  src/mpv/gpu.rs
dcfc8f34e6deb8bad29b32f460f38bbef6f41f6565b1ce9f3e3ff7898316fb1f  src/mpv/library.rs
b4cb2fc59f0a39c40f435f2347a81e5f2996b376ffb694e44ad6170bbc3ccece  src/mpv/mod.rs
e58bab63607d300e780119820c64687ce6cab7fb76f430f65024876e2ed689b1  src/mpv/platform/cgl.rs
3da25a5f862d0d87d1cf6f708ab00b532c9b1ea4344d08ce579c907e7b31ca58  src/mpv/platform/egl.rs
26af4f8c432a3e6d39167783727067e638eaf9d0e169346bb2d56bcf284d41fe  src/mpv/platform/mod.rs
aef51acd95adccf6374022c6e07fb60ba861440b32d8988a8f029fc961bb6380  src/mpv/player.rs
1f32d473347b28beb6a763b3999904158fc2260a0cce77907bc2266e8d09c64b  src/mpv/render.rs
ec0d7fd90fc0bdb4f48c9f807dcca8145c8b5469d10059b5fe8afa8d795db8fd  src/mpv/software.rs
```
