# Watch history and data portability

**Status:** Draft · **Area:** data

## Problem

Personal data lives only in `catalog.sqlite3`. There is no history of when
something was watched, no backup, and no way to move the Library to another
device. Fixes the [known debt in data](../architecture/data.md#locations).

## Current state

The database holds membership, watched flags and resume checkpoints (see
[data](../architecture/data.md#personal-records)). There are no viewing dates,
no import or export, and no sync. Deleting the database loses everything.

## Options

1. **Watch history:** an append-only table of viewing sessions (subject,
   start, end, completion). Keep the existing rule that playback never counts
   as watching unless the 90% rule is met.
2. **Export and import:** a versioned JSON file of membership, watched state and
   progress, keyed by **external IDs**, never local row IDs. Credentials are
   excluded. Import uses the same merge rules as reconciliation (union
   membership, newest watched mutation wins).
3. **Sync:** an optional sync through a cloud or free service, with explicit
   conflict handling. This is a much larger undertaking. Consider it only after
   option 2.
4. **Automatic local backup** of personal tables before migrations.

## Open questions

- Should export include progress checkpoints, which depend on the source?
- How should items that cannot be resolved on the target device be handled
  (for example, a TMDB-only title when TMDB isn't configured)?
