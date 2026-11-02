# Debrid availability and rollout

**Status:** Draft · **Area:** providers

## Problem

Users want to know which streams will play instantly. Kinema currently shows
only whether a release is **already in the user's debrid account**.

## Current state

- Real-Debrid's `instantAvailability` endpoint is disabled upstream (error 37).
  AllDebrid's `magnet/upload` has side effects: it adds the magnet to the user's
  account.
- Kinema makes one read-only listing per configured service and labels the
  result "account presence", not "cached" (see
  [providers](../architecture/providers.md#debrid)).
- Status results, file lists and resolved URLs are fetched again for every
  preparation, with no caching.
- Both adapters exist. Neither has been verified against a live account.

## Options

1. **Account presence only** (today). Honest, with no side effects.
2. **Per-torrent probes.** Upload and then check status. This has side effects
   (it fills the account), hits rate limits, and needs clear user consent and
   cleanup.
3. **A third-party availability service.** This adds a dependency and raises
   privacy and terms concerns.

## Related: validity caches

Use small, bounded **in-memory** caches for debrid status, file lists and
resolved URLs, scoped by provider, account and file identity. A URL can be
reused until the **earlier** of its real expiry and a local deadline that
leaves a safety margin. When the expiry is unknown, reuse it conservatively.
Clear the caches when credentials change or the user logs out. An expired URL
does not invalidate bytes that are already stored for the same file identity.

## Open questions

- In what order should providers roll out, and which capabilities are
  required? Are the built-in adapters enough, or should other services be
  added (Premiumize, TorBox)?
- What does a live acceptance test for each provider look like, without
  putting credentials into tests or logs?
