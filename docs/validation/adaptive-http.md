# Adaptive HTTP acquisition

Scope: production HTTP acquisition only. Direct torrents remain an opt-in POC.
Full-file quota reservations, physical disk admission, durable range checkpoints,
provider identity checks and local-only playback endpoints must remain intact.

## Implemented behavior

`media/adaptive.rs` replaces the serial acquisition loop while retaining one disk
and checkpoint coordinator per asset. Scoped fetch workers use 1–4 parallel
ranges; 16 process-global permits stay attached to payloads until commit/discard.
The existing maximum of eight acquisition owners and full-file quota accounting
are unchanged. The public `MediaService` API is unchanged.

The initial 256 KiB range adapts between 64 KiB and 4 MiB using observed body
throughput; timeouts/body failures reduce the size. Active reader delivery and
contiguous cache-ahead estimates control concurrency with sustained-pressure and
healthy-buffer hysteresis. Local socket throughput is only a demand hint. With
no readers, background completion uses at most two requests. This is not an
optimal-bandwidth estimator, dynamic video bitrate parser or a proven WAN policy.

Strict range validation precedes the unchanged payload sync → manifest checkpoint
→ live range publication ordering. Optional strong ETag binding protects one
acquisition from representation changes, including URL renewal. No initial strong
ETag, or a later acquisition/restart, still relies on trusted provider
MediaIdentity; validators are deliberately not added to manifests. This does not
independently hash/verify HTTP payloads against torrent piece hashes.

Transient retries use bounded exponential backoff. Throttling reduces concurrency
and honors supported Retry-After values; values beyond the 30-second budget fail
explicitly. Cooldown must survive seek generation changes. Coordination is per
asset; shared provider/account-wide rate limiting is not implemented.

## Acceptance and evidence

- Range response validation: reject ignored ranges, incorrect totals/lengths,
  content encoding, truncated bodies and changed content identities.
- Parallel acquisition: bounded in-flight requests and payload memory; reserve a
  range before dispatch; a single owner serializes disk writes and checkpoints.
- Streaming: only durable ranges are visible; out-of-order completions never
  expose holes. New reader priorities supersede obsolete speculative work.
- Adaptation: increase cautiously under buffer pressure, reduce on origin
  throttling, and allow recovery after isolated transient failures. Buffer demand
  is an estimate from local reads, not a reliable media bitrate measurement.
- Recovery: bounded retries and deadlines, cancellation-aware backoff, honor
  supported Retry-After values without busy loops, single-owner signed-URL renewal.
- Lifecycle: owned workers, obsolete result rejection, no lock held across
  network I/O, no credentials or signed URLs in logs/checkpoints.
- Storage: restart trusts durable checkpoints; cancellation and failures retain
  quota reservations until the existing cleanup rules release them.

## Required validation

Run from the repository root with Rust 1.97.1:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
KINEMA_PLAYER_DIAGNOSTICS=1 cargo test -p kinema-desktop --locked media:: -- --test-threads=4
git diff --check
```

Focused local fixtures must exercise parallel completion, seek, transient errors,
throttling, cancellation and persistence without user credentials or public
services. Timing tests should use controlled gates and generous outer deadlines,
not require fragile download-speed ratios.

## Qualification limits

A passing localhost fixture suite does not establish real-provider throughput,
variable-bitrate playback, native player startup/seek experience, WAN fairness,
Windows/macOS runtime behavior or HDR support. Files larger than the configured
cache budget remain rejected; this is not sliding-window disk caching.

## Review and validation status

Lead inspected acquisition coordination, range validation, reader feedback,
reservation ownership, durable publication and the integration fixtures. An
independent read-only review checked lifecycle/security invariants and prompted
strong-validator support and a fix for throttling being bypassed by seek
generations. Review also corrected unconditional concurrency growth into actual
demand-aware hysteresis and added slow-link range sizing.

New controlled fixture coverage includes:

- Out-of-order parallel completion and exact durable payload bytes.
- Interrupted acquisition/reopen without re-fetching durable intervals.
- Two distant readers making progress and superseded successful responses being
  discarded before live publication.
- Multiple services sharing the process-global request budget.
- Transient HTTP/body failures, persistent rate limiting, Retry-After recovery
  and oversized delay rejection.
- Stable strong ETag/If-Range and rejection of changed or missing bound validators.
- Conservative no-reader background concurrency and completed-cache playback
  without renewing an expired signed URL.
- Gated stale-generation 429/503 delays and oversized-delay rejection (this test
  reproduced an early retry before the final implementation correction).

Pure policy tests cover contiguous-ahead feedback, low-demand reduction,
sustained-pressure growth, useful-throughput checks, isolated-error recovery,
range-size bounds, terminal-error precedence and Retry-After parsing/budgets.

### Final lead-run validation — 2026-10-28

Rust 1.97.1, Linux, final combined working tree:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo test --workspace --locked`: **829 passed, 26 ignored**
  (69 core + 651 desktop + 109 mpv passed).
- Diagnostics-enabled media command above: **70 passed**, including stale
  throttling and completed-cache regressions.
- `git diff --check`: passed.

All sources were synthetic loopback fixtures and temporary files. These tests
establish bounded scheduling, correctness and failure-path behavior under their
controlled conditions, not a measured real-provider playback speedup. No new
production dependency or lockfile change was needed beyond the earlier isolated
opt-in torrent POC commit.
