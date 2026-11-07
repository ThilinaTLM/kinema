# External subtitles

**Status:** Draft · **Area:** playback / providers / UI

## Problem

The embedded player can only show subtitle tracks that are muxed into the media.
Many releases ship without the language a user wants, or with subtitles that are
out of sync. Users need to (a) load a subtitle file they already have, and
(b) find one online for the title they are watching.

## Current state

- `kinema-mpv` observes `track-list` and can select, hide, delay and position
  subtitles (`Player::select_subtitle` and friends). There is no `sub-add`, and
  the track parser (`mpv/metadata.rs`) reads only `id`, `type`, `title`, `lang`
  and `selected`, not `external`.
- Media reaches mpv as an opaque loopback URL, so mpv's `sub-auto` sidecar
  matching never applies, and `config=no` disables user mpv config.
- The player is its own OS window (`ui/views/player`). Its subtitle menu is built
  from tracks and is unavailable when the media has no subtitle track.
- GPUI `gpui-pre 0.3.9` routes OS file drops into the element tree as an
  `ExternalPaths` drag on X11, Wayland, Windows and macOS (`on_drop`,
  `drag_over`). The video is a plain `gpu_surface` element, so a handler on the
  player root should receive drops. This is code-read only, not runtime tested.
- `cx.prompt_for_paths` has no file-type filters on any platform; Linux needs
  an xdg-desktop-portal file chooser.
- Playback identity: the coordinator session holds `PlaybackTarget`
  (IMDb ID, plus series/season/episode). `PendingOpen` and the player window do
  not carry it. `PlayLocal` files have no identity.
- No subtitle or audio language preference exists in settings.

## Proposed design

Ship in two phases. Phase 1 is useful alone and builds the load path that
phase 2 reuses.

### Phase 1: local subtitle files

- **mpv:** `Player::add_subtitle(&Path)` submits `sub-add <path> select` as a
  generation-tagged control operation. A load that races a new `loadfile` is
  rejected like other stale commands. mpv drops external tracks on the next
  `loadfile`, which matches per-file intent. Parse `external` (and `codec`) into
  `Track`, so the menu can label added tracks. `external-filename` is a user
  path: never log it.
- **Validation (desktop, off the UI thread):** extension allowlist (`srt`, `ass`,
  `ssa`, `vtt`, `sub`, `sup`), regular file, size cap (for example 16 MiB).
  Non-UTF-8 paths fail explicitly. The file is read in place and never copied
  or deleted. mpv parses the file at `sub-add` time and handles text encoding
  detection; a parse failure surfaces as a player notice, not a silent no-op.
- **Entry points:**
  - Drag and drop onto the player window, with a `drag_over` hint ("Drop to add
    subtitles"). Multiple subtitle files are added in order; the last one is
    selected. Non-subtitle drops get a notice and do nothing.
  - "Add subtitle file…" in the subtitle menu, through `prompt_for_paths` owned by
    the player window (task kept in a field, so it is cancelled with the window).
  - The subtitle menu becomes available whenever a file is loaded, even with no
    tracks, so these actions are always reachable.
- **Not in phase 1:** remembering an added file for the next resume. See open
  questions.

### Phase 2: online search (OpenSubtitles.com)

**Provider facts** (OpenSubtitles admin reply, October 2026,
<https://forum.opensubtitles.com/t/proper-flow-and-quotas-for-integration-into-free-app/8038>;
verify against the API docs before building):

- One consumer API key per application, shipped in the app, is the intended
  model. A descriptive User-Agent with app name and version is required;
  generic ones get blocked.
- Anonymous downloads: 5 per day per IP per key, only if "allow anonymous
  downloads" is enabled on the key. Logged-in users get their own allowance
  (free accounts about 10–20 a day by different sources, VIP 1000); `/login`
  returns the exact figure. Allowances reset at midnight UTC.
- Rate limit: 5 requests per second per IP. Login tokens last 24 hours.
- Every distinct file counts; the same `file_id` again within 3 hours does not.
  Each `/download` response reports remaining downloads and the reset time.
- Search supports `imdb_id`, `parent_imdb_id` + `season_number` +
  `episode_number`, `languages`, `moviehash` and free-text `query`. Download
  returns a temporary link to the file. Hash matches are sparse; combining an ID
  with the hash and comparing release names works best.
- OpenSubtitles.org XML-RPC is shut down for non-VIP users. Do not use it.

**Design:**

- **Identity:** search by the session's `PlaybackTarget` (movie IMDb ID, or
  series IMDb ID with season and episode). Carry the target to the player through
  `PendingOpen` instead of having the window ask for it. Local files without an
  ID offer only a free-text query the user types; never infer identity from the
  file name.
- **Ranking (pure, `kinema-core`):** hash match first, then release-name
  similarity to the playing file name, then downloads and rating, with the
  user's language order and hearing-impaired preference. Ranking may use names;
  identity may not.
- **Moviehash:** compute it only when the bytes are already durable: local files
  on a worker, or cached assets whose first and last 64 KiB are in the range
  manifest. Never force acquisition just to hash.
- **Adapter (`kinema-desktop` providers):** blocking, bounded and cancellable
  like the others, with the shared HTTP client, a bounded byte reader for the
  download, and distinct outcomes: no results, service failure, rate limited,
  quota exhausted (with reset time), credential rejected. No provider trait or
  registry until a second provider ships.
- **Credentials:** the shipped API key is not a user secret but stays out of
  logs. Optional OpenSubtitles username and password go in the OS keyring (new
  account entry). The login token stays in memory and is refreshed on 401.
  Logs carry host, status, counts and categories only, never queries, file
  names, tokens or download links.
- **Storage:** downloaded files go to `cache/subtitles/<hash of provider and
  file id>.<ext>`, with their own small size budget (LRU by mtime, like artwork),
  separate from the media quota. Reusing a cached file avoids spending
  allowance. This is cache, not personal data.
- **UI:** "Search subtitles…" in the subtitle menu opens a panel in the player
  with a language filter and results (release name, language, downloads, HI and
  forced flags, hash-match badge) and the remaining allowance once known. Every
  state is distinct: loading, empty, failed, rate limited, quota exhausted,
  sign-in required. Choosing a result downloads it and calls the phase 1 load path.
- **Settings:** an ordered `subtitle_languages` list (Medium-risk settings key,
  normalized on load) that the user picks in Settings, defaulting to English,
  plus the optional account under provider settings.

## Alternatives considered

- **SubDL:** free per-user API key, search by IMDb or TMDB ID with season and
  episode, `unpack=1` for direct files. A reasonable second provider; its limits
  are not verified. Requiring every user to get a key is more friction than
  OpenSubtitles' shipped key.
- **Stremio OpenSubtitles v3 proxy:** no key, but it is an unofficial
  third-party proxy with unclear terms. Rejected.
- **Addic7ed, Podnapisi, BSPlayer:** scraping or undocumented protocols. Rejected.
- **Automatic sync with `alass`:** a strong Rust tool, but GPL-3.0-or-later,
  which conflicts with linking into Apache-2.0 Kinema. Manual subtitle delay
  already exists. Out of scope.

## Decisions so far

- **Account:** anonymous by default through the shipped key; optional
  OpenSubtitles login in Settings for a personal allowance.
- **Search runs only on request** ("Search subtitles…"), never automatically,
  because every download spends allowance.
- **Languages:** chosen in Settings; default English.
- **Remembering added subtitles across resume:** deferred until phase 1 ships.
  It would be persisted state (likely a SQLite change, High risk).

## Open questions

- Registering the Kinema consumer key with OpenSubtitles (with anonymous
  downloads enabled), and where the key lives in the build.
