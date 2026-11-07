# External subtitles

**Status:** Draft · **Area:** playback / providers / UI

## Problem

Many releases ship without the subtitle language a user wants, or with
subtitles that are out of sync. Users can already add a file they have (see
[playback](../architecture/playback.md#external-subtitles)); they also need to
find one online for the title they are watching.

## Current state

- Local subtitle files are implemented: **Add file** in the subtitle menu and
  drag and drop, loaded through `Player::add_subtitles` and reported as
  `Track::external`. Success is observed (a new external track), not assumed.
  Online search reuses that load path. See
  [playback](../architecture/playback.md#external-subtitles).
- Menu labels come from `kinema_core::tracks`
  ([track labels](../architecture/playback.md#track-labels)): language name
  first, release details from the file name with the playing title removed,
  "File · SRT" in the dimmed column. mpv titles an added track with its file
  name and detects the language only from a trailing tag (`….en.srt`).
- `PlaybackTitle` carries `name` and `episode_name` for display matching only.
- Playback identity: the coordinator session holds `PlaybackTarget`
  (IMDb ID, plus series/season/episode). `PendingOpen` and the player window do
  not carry it. `PlayLocal` files have no identity.
- No subtitle or audio language preference exists in settings.

## Proposed design

### Online search (OpenSubtitles.com)

**Provider facts.** Checked on 2026-11-07 against the
[API reference](https://ai.opensubtitles.com/api-docs) (v1.0.1, mirrors
[opensubtitles.stoplight.io](https://opensubtitles.stoplight.io)), the
[help center](https://opensubtitles.tawk.help/article/getting-started) and an
[admin's forum reply](https://forum.opensubtitles.com/t/proper-flow-and-quotas-for-integration-into-free-app/8038)
(October 2026):

- **Two credentials.** `Api-Key` authorises the *application*; any
  opensubtitles.com account can create one under
  [API Consumers](https://www.opensubtitles.com/en/consumers). A `Bearer` token
  from `POST /login` authorises the *user*. Every request needs the API key and
  a `User-Agent` of the form `AppName vX.Y.Z`; generic agents get blocked.
- **Search** (`GET /subtitles`) is unlimited apart from rate limits. Parameters
  include `imdb_id`, `parent_imdb_id` + `season_number` + `episode_number`,
  `type`, `languages` (comma separated, alphabetical), `moviehash`, `query`,
  `hearing_impaired`, `foreign_parts_only`, `ai_translated` (default include),
  `machine_translated` (default exclude) and `page`. With `moviehash`, results
  carry `moviehash_match` and matches come first. Server ordering is discouraged.
- **Canonical requests avoid redirects:** parameters sorted, lowercase, no
  default values, IDs without leading zeros (`tt0133093` → `133093`).
- **Download** (`POST /download` with a `file_id`) returns a temporary `link`
  (on `www.opensubtitles.com`, valid 3 hours, never cache it), the file name,
  `remaining` downloads and `reset_time_utc`. The file is always UTF-8. The
  allowance is spent on this call, not on fetching the link; the same file again
  within 3 hours is free. Each distinct file counts.
- **Allowance:** anonymous 5 per 24 hours per IP (per key), and only if the
  key's "allow anonymous downloads" setting is on, otherwise `/download`
  answers 401. Logged-in users get their rank's allowance (10 for regular
  accounts per the help center, up to 1000 for VIP); `/login` and
  `GET /infos/user` report it. Counters reset at midnight UTC.
- **Login** is "expensive": at most 1 per second, the token lasts 24 hours, and
  after a 401 the same credentials must not be retried. Later requests must use
  the returned `base_url`, which can be `vip-api.opensubtitles.com` (then the
  token must go with every request).
- **Rate limit:** 5 requests per second per IP.
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
  quota exhausted (with reset time), API key rejected, sign-in required
  (anonymous downloads off) and login rejected. No provider trait or registry
  until a second provider ships.
- **Hosts and redirects:** the shared client follows no redirects, and that
  stays. Send canonical requests (above) so none are needed; treat any redirect
  as a service failure. HTTPS hosts are allowlisted: `api.opensubtitles.com`,
  `vip-api.opensubtitles.com` (only when `/login` returns it as `base_url`) and
  `www.opensubtitles.com` for download links. The link is a bearer capability:
  never log, persist or show it.
- **Credentials:** the user supplies their own **API key** in Settings, like
  the TMDB key; nothing is bundled in the build. Without a key, **Search
  online** stays visible but disabled, and explains where to add one. Key,
  username and password are separate keyring accounts under the
  `kinema` service. The login token stays in memory, is reused for 24 hours,
  and is never retried after a 401 with the same credentials. Because the key
  is the user's own, its "allow anonymous downloads" setting is theirs too:
  a 401 from `/download` without a token reads "Sign in or allow anonymous
  downloads for your key". Logs carry host, status, counts and categories
  only, never keys, queries, file names, tokens or download links.
- **Labels for downloads:** pass the result's release name and language to
  mpv (`sub-add <path> select <title> <lang>`, mpv's optional arguments)
  instead of relying on the cache file name, so the existing label rules show
  "English · 1080p WEB-DL" and the detail could read "OpenSubtitles · SRT".
  This needs an optional title/language on `Player::add_subtitles` and a
  source on `Track` or in the window (mpv's `external-filename` is a cache
  path and must not be shown or logged).
- **Storage:** downloaded files go to `cache/subtitles/<hash of provider and
  file id>.<ext>`, with their own small size budget (LRU by mtime, like artwork),
  separate from the media quota. Reusing a cached file avoids spending
  allowance. This is cache, not personal data.
- **UI:** **Search online** in the subtitle menu opens a **sheet over the
  video**, with a language filter and results (release name, language, downloads, HI and
  forced flags, hash-match badge) and the remaining allowance once known. Every
  state is distinct: loading, empty, failed, rate limited, quota exhausted,
  sign-in required. Choosing a result downloads it and calls the existing load path.
- **Settings:** an ordered `subtitle_languages` list (Medium-risk settings key,
  normalized on load) that the user picks in Settings, defaulting to English.
  An OpenSubtitles section under provider settings holds the API key
  (required for search) and the optional username and password, with a check
  that reports the account's allowance.

## Suggested build order

Each step is independently shippable and testable.

1. **Settings:** `subtitle_languages` (default English) with a Settings
   control, and the OpenSubtitles API key plus optional login in the keyring,
   validated through `/infos/user` or `/login`.
2. **Adapter** with loopback-fixture tests: search, download link, the bounded
   file download, and every distinct failure (empty, service, rate limited,
   quota with reset time, credential rejected, disabled anonymous downloads).
3. **Identity to the player:** carry `PlaybackTarget` through `PendingOpen`
   (catalog playback only; local files use a typed query).
4. **Search panel** in the player, with the subtitle cache and load path, and
   design-lab scenes for every state.
5. **Moviehash** for local files and fully cached head/tail ranges, as a
   ranking boost.

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

- **API key:** the user configures their own OpenSubtitles API key in
  Settings. Nothing is bundled in the build. Without a key, online search is
  unavailable and says so.
- **Account:** the key alone gives anonymous downloads (if the key allows
  them); an optional OpenSubtitles login in Settings gives a personal allowance.
- **Search UI:** a sheet over the video. Playback keeps running behind it;
  the user pauses manually if they want to.
- **AI-translated subtitles** stay included (the API default) and carry an
  "AI translated" badge. They rank below human subtitles in the same language,
  so they matter mostly when no human subtitle exists. Machine-translated
  subtitles stay excluded (also the API default).
- **Keyring:** the service was renamed to `kinema` first.
- **Search runs only on request** (**Search online**), never automatically,
  because every download spends allowance.
- **Languages:** chosen in Settings; default English.
- **Remembering added subtitles across resume:** not built; local files are
  per load. Revisit after online search. It would be persisted state (likely a
  SQLite change, High risk).

## Open questions

- None blocking. Settle details (sheet layout, result row content) with a
  wireframe or design-lab scene during step 4.
