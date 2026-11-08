# Glossary

Project terms, and ordinary words that mean something specific in Kinema. Docs,
proposals and code reviews use these terms exactly as defined here.

| Term | Meaning here | Not to be confused with |
| --- | --- | --- |
| Account presence | A release is already in the user's debrid account, from one read-only listing ([providers](providers.md#debrid)) | "Cached on debrid", instant availability or playback readiness |
| Acquisition | Downloading an asset's bytes from an upstream URL into managed storage, run by one coordinator per asset ([media](media.md#acquisition)) | A player reading bytes from the loopback server |
| Active (provider) | A provider that is enabled and has the credentials it needs ([providers](providers.md#metadata-ordered-priority-and-enrichment)) | Primary; an active provider may only enrich |
| Asset | One managed payload file plus its manifest, identified by upstream file identity ([media](media.md#storage)) | A session or lease; several leases can share one asset |
| Canonical entity | A provider-independent title, season or episode row in the catalog ([data](data.md#table-groups)) | Provider page membership or a provider's raw record |
| Checkpoint (media) | Making downloaded ranges durable: sync the payload, then atomically replace the manifest ([media](media.md#storage)) | A progress checkpoint |
| Checkpoint (progress) | The saved resume position for a movie or episode ([playback](playback.md#progress-and-resume)) | Watched state; a media checkpoint |
| Cinema Dock | The embedded player's bottom chrome: time row, seek rail, transport and menus ([playback](playback.md#player-chrome-and-input)) | A floating card; it is an edge-attached scrim |
| Cinema palette | `Palette::cinema()`, the always-dark palette used by player windows ([design system](design-system.md)) | The app's Dark mode |
| Discover / Library (scope) | The two Browse scopes: provider catalogs vs. titles the user saved ([UI](ui.md#library)) | Downloaded files, which are not the Library |
| Enrichment | Filling missing fields from lower-priority providers after the primary result ([providers](providers.md#metadata-ordered-priority-and-enrichment)) | Replacing primary values; per-field priority still wins |
| Epoch | A durable per-item counter bumped by each new playback run or a progress clear; it fences stale progress writes ([playback](playback.md#progress-and-resume)) | A generation, which lives in memory only |
| Generation | A monotonic in-memory number per request slot, screen or session, used to reject late results ([runtime](runtime.md#background-work-rules)) | Cancellation, which saves work but doesn't guarantee correctness |
| Glass | The translucent header material from `palette.glass()` ([design system](design-system.md)) | Any blur effect; it is a translucent fill |
| Known debt | An implemented decision documented as bad or outgrown, with a proposal to fix it ([rule strength](README.md#rule-strength)) | A proposal, which is not built |
| Kept preparation | A debrid preparation (file list, provider) held by an embedded player window after start, so other files of the pack can play into it ([playback](playback.md#episode-swap-same-window)) | A download or a cached file; it holds no bytes |
| Lease | A playback session's claim on an asset or local file; releasing it is idempotent ([media](media.md#rules)) | An asset; a player instance (external replay can share a lease) |
| Membership | A title is in the user's Library (a `library_entries` row) ([data](data.md#personal-records)) | Watched state or progress; all three are independent |
| Neighbour | The previous or next episode of the one playing, from the canonical listing, skipping specials and crossing seasons ([playback](playback.md#choosing-the-neighbours-stream)) | An episode guessed from file names |
| Primary (provider) | The first active provider in the priority list; it owns search and browse ([providers](providers.md#metadata-ordered-priority-and-enrichment)) | The only provider that contributes data |
| Provenance | The supplier, resource and observation time of a field's current value ([data](data.md#merge-rules)) | A history of values; only the current one is kept |
| Run | One playback of a subject, with its own write authority for progress ([playback](playback.md#progress-and-resume)) | A session, which is about bytes and leases |
| Scrim | A text-bearing dark backing over artwork (`scrim`, `chip_on_art`) | `overlay`, which only dims and guarantees no contrast |
| Selection | The `selection` fill plus a marker showing a chosen item | Focus, which is the ring and is independent |
| Session | A media session, which is a lease plus its local URL ([media](media.md#rules)) | A playback run or a player window |
| Slot | A queue position in the catalog worker per operation, kind or indexer; it holds at most one queued request ([runtime](runtime.md#catalog-worker)) | A thread |
| Stream memory | The last played stream per episode and release per season, a personal record of raw indexer strings ([data](data.md#stream-memory)) | Progress or watched state; a cache |
| Strong match | A candidate release with the same group and resolution as the reference; the only kind chosen without the user ([providers](providers.md#indexers)) | Same pack, which is the same torrent |
| Subject | The typed identity whose progress is saved: `Movie`, `Episode` or `Untracked` ([playback](playback.md#progress-and-resume)) | A display title or session ID |
| Toast | A top-right card for an outcome or a page condition, derived from state each render and keyed by slot; each occurrence is also kept in the bell's notification log ([UI](ui.md#toasts)) | An inline `Alert`, a native OS notification |
| Token (design) | A named value in `ui/theme` (color, size, spacing, opacity, timing) ([design system](design-system.md)) | Credentials or API tokens |
| Watched | A manual or 90%-rule flag on a movie or episode; seasons and shows derive it ([data](data.md#personal-records)) | Progress, membership or download completion |
