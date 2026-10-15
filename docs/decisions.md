# Open decisions

The [architecture](architecture.md) is the single summary of agreed scope. Implementation notes are labeled separately from the proposed fuller schema, file layout, and policies. Discuss questions **one at a time**; don't turn this list into an implementation backlog.

## Next: who can reach the media server?

Should the MVP serve players only on the same computer, or also other devices on the local network?

**Recommendation:** same-computer/loopback only initially. LAN support introduces authentication, firewall exposure, and different server-lifetime expectations. It must be intentional, not an accidental bind to every interface.

No answer has been recorded yet.

## Remaining choices

| Area | Still open |
| --- | --- |
| Media storage | Default quota, eviction order, unknown-size files, lowering the limit, and a custom cache location |
| Metadata | Global preferred provider/fallback rules, automatic refresh intervals, and additional free providers with suitable terms |
| Response caches | Exact TTLs, retained-entry limits, and offline use of stale results |
| Provider integrations | Rollout order for debrid providers, required capabilities, and whether built-in adapters are sufficient |
| External players | Discovery/configuration, reconnect grace, and app/server shutdown behavior |
| Desktop releases | Minimum OS versions, architectures, packaging, accessibility, and optional OS integrations |
| Performance | Representative workloads and numerical responsiveness/resource budgets |

## Not MVP work

Personal library/progress, embedded playback, and a built-in torrent engine are planned later. Android/iOS, explicit retained downloads, runtime plugins, LAN sharing, transcoding, and rolling caching are not current commitments. Leave useful boundaries for future work; do not implement unused subsystems.

## Recording an answer

Update the relevant document with the decision and its reason, then remove the resolved question here. Keep one description of each rule rather than copying it into every page. No migration of the old configuration, database, or keyring is planned.
