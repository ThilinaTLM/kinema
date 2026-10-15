# Architecture

## What we are building

Kinema Next is a local-first movie and TV-series application written in Rust with GPUI. It targets **Windows, macOS, and Linux from the first release**, without KDE dependencies.

### Agreed MVP

- Browse/search a locally cached movie and episode catalog, using TMDB/Cinemeta metadata.
- Find and select sources through Torrentio and Peerflix.
- Integrate debrid services, targeting Real-Debrid and AllDebrid.
- Download media into managed local storage and serve it through Kinema's media server while downloading.
- Open local server URLs in external VLC or standalone mpv. Also serve existing local files.
- Cache metadata and indexer results; show last-update times and provide force-refresh.
- Enforce a user-configurable media-cache limit. Warn and offer to increase it when one file exceeds the limit.

**Later:** personalized library membership, watch history/progress and tracked resume, embedded playback, and a built-in torrent engine. Rolling cache eviction is optional, not required for the MVP. Mobile support is only a future possibility.

The catalog is not a personalized library. Media downloading is not proof that someone watched it.

## How it fits together

This diagram shows responsibilities and data flow, not separate processes.

```mermaid
flowchart TB
    UI["GPUI screens"] --> Catalog["Catalog queries + sync"]
    Catalog --> DB[("SQLite")]
    Catalog --> Metadata["TMDB / Cinemeta"]
    UI --> Sources["Source discovery"]
    Sources --> DB
    Sources --> Indexers["Torrentio / Peerflix"]
    UI --> Playback["Playback coordination"]
    Playback --> Acquire["Media acquisition"]
    Acquire --> Debrid["Real-Debrid / AllDebrid"]
    Debrid -->|"Media bytes"| Storage["Managed files + available ranges"]
    Acquire -.-> Torrent["Future torrent engine"]
    Torrent -.-> Storage
    Playback -->|"Local URL"| Player["External VLC / mpv"]
    Player -->|"HTTP range requests"| Server["Local media server"]
    Server --> Storage
    Server -->|"Request missing ranges"| Acquire
    Local["Existing local files"] --> Server
```

Players receive local server URLs, not upstream debrid URLs. The player decodes the video; the server delivers bytes. Transcoding, LAN sharing, and a standalone daemon are not assumed requirements.

TMDB and Cinemeta supply metadata. IMDb IDs identify titles in much of the source workflow; there is no direct IMDb catalog integration. A title, a particular release/file, and an expiring playback link are different identities.

## Keep the implementation small

Use one application with these boundaries:

| Part | Responsibility |
| --- | --- |
| Presentation | GPUI views, navigation, loading/error states, and user actions |
| Application | Catalog sync, source lookup, acquisition, and playback coordination |
| Domain | Typed identities and pure rules such as episode/file selection |
| Adapters | Provider APIs, SQLite, filesystem, secure credentials, and OS integration |

Construct dependencies in one place. Keep GPUI, SQL, and provider response formats out of domain rules. Provider adapters should be easy to add or replace; runtime plugin loading is not needed to achieve that.

Start with a small in-memory sync queue, one SQLite worker, and bounded background work. No persisted job system, global event bus, or general plugin framework is needed. Exact crates/libraries remain to be selected.

## Responsiveness and lifetime rules

- Read local data first. Fetch missing data; proposed background refresh keeps stale-but-usable results visible. See [catalog sync](catalog-schema.md) and [cache policies](cache-policy.md).
- Keep SQL, network waits, image decoding, and blocking file operations outside UI rendering.
- Give background tasks an owner. Cancel obsolete work and reject late results using request/session generations; these solve different problems.
- Screen state belongs to its navigation entry. Closing a page must not stop an external player or remove files it is using.
- Prioritize interactive work, especially seek requests, over prefetch. Bound queues and buffers.
- On shutdown, stop accepting work, cancel requests, checkpoint media availability, and close resources in order. External-player/server lifetime needs an explicit policy.
- Distinguish empty results, unavailable services, missing credentials, and storage failures. Errors must not silently become “no data.”

External launch success and HTTP traffic do not establish playing/paused/completed state. Real player telemetry will be needed when watch tracking is introduced.

## Desktop support and security

Use native adapters or maintained cross-platform libraries for application directories, secure credential storage, player discovery/launch, and optional desktop features. Store secrets in the OS secure store, never settings, SQLite, manifests, or logs. Launch players with argument lists, not constructed shell commands.

Test on all three desktops throughout development. Pay particular attention to Unicode paths, file locks/deletion, secure-store errors, player reconnects, and GPUI window/rendering behavior; test Wayland/X11 on Linux. Minimum OS versions, architectures, packaging, and optional integrations remain open. Pin and validate a GPUI revision rather than assuming framework support is sufficient.

## What we learned from the old application

Keep explicit composition, stable content identity, player/source boundaries, pure policies, and stale-response tests. Avoid shared detail state being overwritten by navigation, blocking work on the UI path, detached tasks without lifecycle rules, and database failures disguised as empty collections.

The [feature inventory](features.md) links to the reference implementation. These are source-level findings, not measured performance claims.

## First implementation slice

Select a movie, find a source, resolve it through one debrid provider, download into managed storage, and launch VLC/mpv with the local URL. Test seeking to missing bytes, repeated local metadata reads, interrupted downloads, refresh failures, and cache exhaustion on all desktop targets. Then cover episodes and both indexers before calling the MVP complete.

Measure UI stalls, result latency, seek recovery, startup, memory, and disk usage. Use those results to justify complexity—not assumptions that rewriting in Rust automatically makes the application faster.
