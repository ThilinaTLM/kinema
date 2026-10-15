# Open decisions

The [architecture](architecture.md) is the single summary of agreed scope. Implementation notes are labeled separately from the proposed fuller schema, file layout, and policies. Discuss questions **one at a time**; don't turn this list into an implementation backlog.

## Next: media storage behavior

The quota default, manual inactive-only cleanup, unknown-size admission, and
limit-lowering safety rules are agreed in [storage policy](storage-layout.md#limits-and-cleanup).
A custom cache location and checkpoint intervals remain open. Discuss one question
at a time; no additional behavior is resolved here.

## Remaining choices

| Area | Still open |
| --- | --- |
| Media storage | A custom cache location and checkpoint intervals |
| Metadata | Automatic refresh intervals, multi-locale behavior, and additional free providers with suitable terms; explicit Cinemeta/TMDB selection with no automatic fallback is [agreed](architecture.md#agreed-metadata-provider-contract) |
| Response caches | Exact TTLs, retained-entry limits, and offline use of stale results |
| Provider integrations | Rollout order for debrid providers, required capabilities, and whether built-in adapters are sufficient |
| External players | Discovery/configuration and reconnect grace; quit/close confirmation and server lifetime are [agreed](architecture.md#agreed-session-lifetime-policy) |
| Desktop releases | Minimum OS versions, architectures, packaging, accessibility, and optional OS integrations |
| Performance | Representative workloads and numerical responsiveness/resource budgets |

## Not MVP work

Personal library/progress, embedded playback, and a built-in torrent engine are planned later. Android/iOS, explicit retained downloads, runtime plugins, transcoding, and rolling caching are not current commitments. LAN sharing is excluded; the MVP server is loopback-only. Leave useful boundaries for future work; do not implement unused subsystems.

## Recording an answer

Update the relevant document with the decision and its reason, then remove the resolved question here. Keep one description of each rule rather than copying it into every page. No migration of the old configuration, database, or keyring is planned.
