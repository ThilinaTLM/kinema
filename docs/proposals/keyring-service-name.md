# Keyring service name

**Status:** Accepted · **Area:** providers

## Problem

Credentials are stored under the keyring service name `kinema-next`, which is
left over from the rewrite. The name is a persisted contract: changing it
naively orphans every user's stored tokens. Fixes the
[known debt in providers](../architecture/providers.md#credentials).

## Current state

There is one keyring entry per account under `kinema-next` (`credentials.rs`).
Legacy C++ keyring entries are never migrated, and that stays true.

## Options

1. **Rename to `kinema` with a one-time copy.** At startup, read each known
   account from `kinema-next`, write it under `kinema`, verify, then delete the
   old entry. Each step must handle a locked or unavailable store, and a crash
   between steps.
2. **Rename, and ask users to re-enter tokens.** Simple, but it costs every
   user a setup step. It is acceptable only before 1.0.
3. **Keep the name,** and document it as permanent.

## Decision

Rename the service to **`kinema`** with option 2: Kinema is pre-1.0, so users
re-enter their tokens once, and there is no copy step or shim. Do it before
the OpenSubtitles credentials are added (see
[external subtitles](external-subtitles.md)), so no new entry is ever written
under `kinema-next`.

When implementing:

- Change the one service name in `credentials.rs` and the
  [providers](../architecture/providers.md#credentials) page, then delete this
  proposal and its Known debt note.
- Existing tokens then read as **missing**, the normal "not configured" state,
  never as a failure. Settings already prompts for missing keys.
- Leave the old `kinema-next` entries alone; Kinema does not delete what it no
  longer owns. Say so in the release notes.
- **Legacy collisions: checked, none.** The legacy C++ app stored its tokens
  in KWallet folders (`dev.tlmtech.kinema`, `kinema`) with different entry
  names (`realdebrid-token`, `tmdb-token`, …). It did not use Secret Service
  items with `service`/`username` attributes, which is what the `keyring` crate
  reads, so `kinema` cannot pick them up. On the maintainer's machine a Secret
  Service search for `service=kinema` found nothing even while those folders
  existed, and the legacy folders have since been deleted. Legacy entries are
  still never read or migrated.
