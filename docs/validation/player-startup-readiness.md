# First-play readiness timeout correction

## Report and source evidence

The reported player showed “Saved position cannot be resumed” while acquisition
was progressing. The user had never watched the episode and reported that
“Play from beginning” worked. This does not prove a saved checkpoint existed or
that acquisition failed.

Source inspection found that the progress protocol used the same Unavailable
state and resume wording for initial startup, missing position/seekability, and
failed seek confirmation. Initial Loading timed out after ten seconds even for
first playback, and timeout evaluation preceded handling the newest backend
snapshot. A slow startup could therefore become permanently Unavailable before
late valid readiness was processed. These are confirmed code paths; no runtime
trace establishes which individual condition expired in the reported session.

## Implemented correction

- Initial Loading/AwaitSeekability now has a bounded **60-second** budget;
  confirmation of an already-issued seek retains the strict **10-second** budget.
- Valid current readiness is processed before expiring that attempt on the same
  tick. First-play readiness still requires a real finite, nonnegative observed
  position and the existing valid loaded/generation state.
- With no usable checkpoint, an unavailable attempt says **“Playback could not
  start”**, not “Saved position cannot be resumed.” Real saved-checkpoint flows
  retain their resume choices and safeguards.
- Fresh generation, position/restart revisions and persistence authorization are
  preserved. No fabricated progress or automatic reset of stored history.
- Explicit Unavailable/Choice/Mismatch decisions are not automatically dismissed
  by late events. Retry still begins a fresh bounded attempt.
- A timeout transition logs only fixed categories and readiness flags, not media
  identifiers, URLs, saved positions or credentials.

## Validation

Lead reviewed both changed protocol/UI files, including timeout evaluation order,
first-play wording, generation binding and unchanged strict seek confirmation.
Focused regressions cover readiness after 11/45/60 seconds, a still-bounded initial
wait, saved-checkpoint preservation, invalid positions, no premature writes,
explicit decision preservation and seek confirmation at the deadline.

Final lead-run checks passed on Linux with Rust 1.97.1 (2026-10-28):

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p kinema-desktop --locked playback_progress::tests
git diff --check
```

Workspace: **833 passed, 26 ignored**; focused progress protocol: **17 passed**.
An initial more-specific test filter matched zero tests; the corrected command
above was rerun and actually executed all 17 protocol tests.

Tests use synthetic backend states, not the user's database, credentials, media
or live provider. No existing user progress is modified by this investigation.
Native reproduction of the reported source remains unverified.
