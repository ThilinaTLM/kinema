# Welcome wizard

**Status:** Draft · **Area:** UI

## Problem

A new user has to find Settings on their own to add a TMDB token, a debrid key
and a cache limit before streams work.

## Current state

Setup happens only in the Settings window (General, Accounts, Playback, Storage
and the other sections in [UI](../architecture/ui.md#transfers-settings-quit)).
Missing credentials show 🔑 plus "Open Settings" where they block a screen.

## Recommendation

A skippable, first-run sheet (or a sequence of sheets) that leads the user
through these steps:

1. Metadata: Cinemeta works without a token, and TMDB is optional. The TMDB
   step explains the API Read Access Token and the TMDB attribution.
2. Debrid: choose Real-Debrid or AllDebrid, paste the key, then **Test**. The
   key is stored only in the OS keyring.
3. Playback: check that libmpv is present, and say clearly what is missing.
4. Storage: the media limit (default 20 GiB) and its location.

Every step reuses the existing Settings sections and their actions, so there is
one implementation. Show the wizard again only on request, never automatically
after it has been completed or skipped.

## Open questions

- Should the wizard also come up when the keyring is unavailable, to explain
  how to fix it?
