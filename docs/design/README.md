# Desktop design

[Desktop v3](desktop-v3.md) is the sole/default native UI contract: direct GPUI
and in-house widgets, without Kit or UI-version switches. Approved revisions
supersede conflicting portions of the base contract. Design approval and source
implementation do not establish full workflow or cross-platform acceptance.

| Document | Purpose |
| --- | --- |
| [Desktop v3](desktop-v3.md) | Layouts, components, accessibility, motion and performance requirements |
| [UI revision](ui-revision-2026-10.html) | Approved toolbar, detail, Streams and catalog design revisions |
| [Episodes revision](episodes-revision-2026-10.html) | Approved episode, season and shared artwork/control revisions |
| [Player](player.md) | Independent native player, controls, ownership and scoped evidence |
| [Playback progress and desktop integration](playback-progress-desktop.md) | Approved resume persistence, OS media controls, inhibition, and tray lifecycle; scoped source validation |
| [Playback progress in context](playback-progress-in-context.html) | Reviewed screenshot-based placements in series, episode grid, Streams and movie detail |
| [Playback progress state wireframes](playback-progress-wireframes.html) | Supporting watch/resume behavior and tray lifecycle illustrations, not current-layout placement |
| [Player controls wireframe](player-controls-wireframes.html) | Approved Cinema Dock visual reference |
| [Library](library.md) | Personal membership and manual watch-state contract |
| [Brand](brand.md) | Mark, icon, color and usage guidance |
| [Native UI lab](ui-probe.md) | Offline fixture commands, capture tooling and validation limits |

HTML files are design artifacts, not shipped application UI or native acceptance
evidence. Superseded Graphite v2 prototypes, Kit implementation guides and stale
handoffs have been removed. Still-applicable collection/artwork requirements are
in [Desktop v3](desktop-v3.md#retained-collection-and-artwork-acceptance-requirements);
visual-baseline review requirements are in the [probe guide](ui-probe.md#visual-baseline-review-requirements).

See [development](../development.md#validation-status-and-limits) for current
validation boundaries and native prerequisites. GPU-surface default-on does not
qualify live-provider workflows, native Windows/macOS playback or HDR.
