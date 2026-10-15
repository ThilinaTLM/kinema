# Old application feature inventory

This is a reference for the old C++/Qt application, **not a parity checklist** for the Rust/GPUI redesign. See [architecture](architecture.md) for confirmed MVP scope and future work. Old settings, database and keyring entries will not be migrated.

Implementation and tests describe intended behavior; they do not guarantee that live third-party services still work.

## Metadata and discovery

- TMDB supplies discovery rails and filtered, paginated browsing; it requires a token.
- Cinemeta is the primary text-search and title-detail source. Empty searches can fall back to token-enabled TMDB search; request failures are not empty results.
- Search accepts IMDb IDs and title URLs. Series details expose seasons, episodes and episode-specific sources.
- TMDB discoveries must map to IMDb before source lookup. Title/episode identity is separate from release/file identity, so progress can survive a release change.

## Sources and debrid

- Settings select **one active indexer**, Torrentio or Peerflix; results are not federated.
- Results may include release quality, size, seeders, codec/audio hints and file identity. Parsed labels are hints, not verified media properties.
- Shared exclusions, picker filters and sorting help narrow results. Some categories cannot be inferred locally; language filtering depends on provider data. **Enforced cached-only filtering is not established** by the old implementation.
- Actions include play/download and copying or opening release links. Selecting the correct episode in a pack matters; largest-file or positional guesses are not reliable.
- Real-Debrid and AllDebrid resolve magnets, wait for metadata/readiness, select files and unlock links with bounded waits. Real-Debrid does not use the deprecated instant-availability endpoint.
- Resolvers retain file catalogs for pack navigation. Playback URLs can expire: stable source/file references, not temporary authorized URLs, are the durable identity.
- Debrid-backed transfers use a shared local cache/range gateway; URL-only sources can go directly to a player. Debrid failure reports an error rather than silently switching to local torrenting.

## Playback and subtitles

- Optional embedded mpv provides play/pause, seeking, volume, speed, fullscreen, keyboard controls, track/chapter selection and local subtitle attachment.
- Embedded playback supports resume prompts, remembered track languages and chapter-based intro/credits skipping. Position, duration and terminal events feed history; EOF, stop, replacement and failure are distinct.
- Pack catalogs support previous/next episodes and auto-next on natural EOF when an adjacent episode is identified. This is **same-pack navigation**, not universal next-episode discovery; stop/failure does not auto-next.
- External mpv, VLC or custom commands are detached hand-offs: no progress telemetry, reliable app-controlled resume, transport/track control or subtitle attachment. Kinema cannot stop the detached player; launch success is not completion.
- OpenSubtitles supports title/episode, language and release filters plus best-effort moviehash matching. Downloads require API key and account credentials, expose quota and use a bounded subtitle cache. Embedded playback can attach them; external integration is not equivalent.

## History, library and Up Next

- History tracks title/episode progress, stable last-source references and track-language preferences. Continue Watching excludes completed items and deduplicates series.
- Completion uses EOF/progress/credits policies, not simply opening a source. Movies and episodes can also be marked watched manually.
- Library membership and watched history are separate; removing a saved title does not erase history. Library browsing has filters and sorting.
- Up Next groups saved titles into ready-to-watch, airing-soon and recently-added rails. It is distinct from player auto-next.

## Downloads, local torrenting and offline use

- Downloads support pause/resume, retry, cancellation, pinning and removal with optional file deletion. Retained work can be restored at startup; streaming and saving offline can reuse the same asset.
- The local libtorrent backend selects files and schedules pieces/ranges behind a localhost HTTP gateway. It is separate from debrid and exposes the user's connection to torrent peers; using it must be explicit.
- Offline playback means retained local bytes, not a debrid cached badge or a partially streamed file. It does not promise offline metadata discovery or source resolution. Cleanup distinguishes temporary data from pinned downloads.

## Desktop and credentials

- The old Linux desktop integration includes Qt/KDE theming, keyboard navigation, tray/close behavior, notifications, idle inhibition and embedded-player MPRIS/media keys. These are historical behaviors, not requirements for the redesign.
- Settings cover providers, filtering, players, subtitles and transfer/cache limits. Secrets go through `TokenController`/`TokenStore` into the system keyring, not ordinary settings.
- Missing/rejected credentials and unavailable keyring storage need distinct states. Credentials must stay out of logs and shared links; credential-bearing Torrentio URLs are reduced to safe source/file references before link actions.

## Selected implementation references

- **Discovery:** [search and IMDb mapping](../reference/cpp/src/ui/qml-bridge/search/SearchViewModel.cpp), [title actions](../reference/cpp/src/ui/qml-bridge/details/TitleActions.cpp); [search tests](../reference/cpp/tests/tst_search_view_model.cpp).
- **Sources:** [indexer selection](../reference/cpp/src/api/indexers/IndexerSelector.cpp), [filters](../reference/cpp/src/core/util/StreamFilter.cpp), [RD](../reference/cpp/src/playback/sources/RealDebridResolver.cpp) / [AD](../reference/cpp/src/playback/sources/AllDebridResolver.cpp); [file-selection tests](../reference/cpp/tests/tst_media_file_selection_policy.cpp).
- **Playback:** [embedded adapter](../reference/cpp/src/ui/player/EmbeddedMpvPlayerAdapter.cpp), [external adapter](../reference/cpp/src/playback/adapters/ExternalPlayerAdapter.cpp), [subtitles](../reference/cpp/src/playback/subtitles/SubtitleSessionService.cpp); [pack-navigation tests](../reference/cpp/tests/tst_series_session_service.cpp).
- **Personal state and media:** [library](../reference/cpp/src/ui/qml-bridge/library/LibraryViewModel.h), [transfers](../reference/cpp/src/playback/transfer/TransferUseCase.h), [local gateway](../reference/cpp/src/playback/streaming/LocalHttpStreamGateway.cpp); [history tests](../reference/cpp/tests/tst_history_store.cpp), [transfer tests](../reference/cpp/tests/tst_transfer_use_case.cpp).
- **Desktop and secrets:** [shell](../reference/cpp/src/ui/qml-bridge/shell/ShellViewModel.cpp), [tokens](../reference/cpp/src/controllers/TokenController.cpp); [token tests](../reference/cpp/tests/tst_token_controller.cpp), [URL-redaction tests](../reference/cpp/tests/tst_url_redactor.cpp).
