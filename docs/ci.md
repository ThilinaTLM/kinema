# Continuous integration

[The workflow](../.github/workflows/ci.yml) runs on pushes to `main` and pull
requests whose **base branch** is `main`. Feature-branch pushes do not start a
second copy of the PR checks. Pushing a new commit to an open PR still starts PR
checks; concurrency cancels the obsolete run for that PR. Pushes to `main` have
a separate concurrency group. Check names and the three native OS jobs remain
stable for branch-protection rules. No path filters skip required checks.

## Coverage and duration

Formatting and asset validation run independently of the Linux, macOS and
Windows native jobs. Each native job retains:

- Strict workspace/all-targets Clippy with default features. This also checks
  compilation, so a separate, preceding `cargo check` is unnecessary.
- Default-feature workspace tests.
- Dedicated `ui-probe-test` Clippy (library + probe binary) and library tests.
  These are intentionally separate: enabling test support throughout would no
  longer check the same default-feature configuration.

CI alone disables incremental compilation and dev/test debug symbols using job
scoped environment variables. The existing optimization levels and features are
unchanged; normal local builds are unaffected. Less debug information reduces
artifact/link/cache overhead, but failed CI binaries have no debug symbols.
Reproduce locally with these variables unset when debugging a failure.

## Two cache layers

- `Swatinem/rust-cache@v2` retains Cargo downloads and eligible dependency build
  artifacts. Its default cleanup excludes packages under the workspace root,
  including our vendored path dependencies. `cache-workspace-crates` alone does
  not include those patches, which are not workspace members.
- `mozilla-actions/sccache-action@v0.0.9` installs pinned sccache `v0.19.0`.
  `RUSTC_WRAPPER=sccache` and `SCCACHE_GHA_ENABLED=true` enable content-addressed
  compiler caching through GitHub's cache service, including eligible vendored
  library compilations. Incremental Rust builds are disabled for compatibility.
  Linking, build scripts and some compiler invocations are not cacheable; this
  does not eliminate all compilation. The action reports statistics in its
  post step. Both layers follow GitHub's cache access/scoping rules; PR caches
  are not a substitute for a warm default-branch cache.

The first run after these changes may be cold. No hosted-run duration or cache
hit-rate improvement has yet been measured. Compare several equivalent runs on
each OS, separating cold and warm caches. Inspect:

1. Overall PR critical path (the last required check), not summed job minutes.
2. Dependency/tool installation, cache restore/save, Clippy, tests and UI-probe
   step durations.
3. sccache hits/misses, non-cacheable reasons and cache errors in post-step stats.
4. Whether reduced compile time exceeds added compiler-cache network overhead.

Keep all three OS jobs and both feature configurations when tuning further.
Do not infer GUI/playback support from passing compilation/tests; see the
[cross-platform qualification matrix](validation/cross-platform.md).

## Workflow validation

Run `actionlint .github/workflows/ci.yml` after workflow changes. The normal
[repository checks](development.md) still apply. Hosted runner installation,
GitHub cache access and Windows/macOS execution must be verified by an actual
workflow run; local YAML lint and Linux checks cannot establish those results.

Local validation on 2026-10-28: actionlint 1.7.7, trigger/matrix/command assertions,
root formatting and diff checks passed. Strict workspace Clippy and workspace
tests passed with the CI incremental/debug settings (742 passed, 25 ignored).
Separately, sccache 0.19.0 with Rust 1.97.1 passed core Clippy/tests using its local
disk backend; a repeated Rust library compilation produced a cache hit. These
checks do not validate the GitHub cache backend or hosted-run durations.
