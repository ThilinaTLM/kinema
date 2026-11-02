# Keyring service name

**Status:** Draft · **Area:** providers

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

## Recommendation

Option 2 if it happens before the first release, otherwise option 1.

## Open questions

- Should the service name include the app ID (`dev.tlmtech.kinema`) for clarity
  on macOS and Windows?
