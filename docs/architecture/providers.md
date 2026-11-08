# Providers and credentials

Kinema talks to three kinds of external service. Each kind sits behind its own
small adapter boundary.

| Kind | Services | Code |
| --- | --- | --- |
| Metadata | Cinemeta (no token), TMDB (user bearer token) | `metadata_providers/`, `tmdb.rs`, `catalog_normalized_tmdb.rs` |
| Indexers | Torrentio, Peerflix (public, no credentials) | `sources.rs`, `kinema-core::sources` |
| Debrid | Real-Debrid, AllDebrid (user API token) | `providers/` (endpoint notes in `providers/README.md`) |

Adapters are **blocking, bounded and owner-driven**. They create no tasks. The
caller runs them on a bounded worker, owns cancellation (`&AtomicBool`), and
checks the generation of the result. Adapters are compiled in. There is no
runtime plugin loading, and adding a provider means adding an adapter behind
the same boundary.

## Metadata: ordered priority and enrichment

- Settings hold an **ordered provider list**. The default is Cinemeta, then TMDB
  (`kinema-core::provider::ProviderPriority`). The first enabled, *active*
  provider is the **primary** provider and owns search and browse. The others
  only **enrich**.
- A provider is inactive when its required credential is missing. It is shown
  as inactive and skipped. That is not an error, and it never erases local rows.
- Each provider declares its capabilities (search, browse/discover, title
  detail, season/episode, external-ID lookup). It normalizes its output into
  `kinema-core::catalog_normalized` observations (`TitleObservation`,
  `FieldValue`, `RatingObservation`, `ExternalIdentifier`). The
  [merge rules](data.md#merge-rules) decide what is stored.
- **Detail** shows the primary result first, then enriches it asynchronously
  from the other providers. **The grid** enriches only *visible* cards, and only
  fields that are missing (poster, year, rating, runtime, genres). It works with
  bounded concurrency, skips titles that are already enriched, and cancels when
  the user scrolls or changes the query. Enrichment failures are quiet.
- Detail pages show a footer that names only the providers that actually
  contributed, such as "Data from TMDB and Cinemeta". There is no per-field
  provenance UI.
- Identity uses explicit IDs only: IMDb mappings, TMDB `find` by IMDb ID, and
  `external_ids`. **Source lookup needs an IMDb ID.** If a TMDB title has no IMDb
  mapping, its sources show as unavailable. They are never guessed.
- **TMDB terms:** users supply their own API Read Access Token, and the project
  embeds no key. The About page must show the approved logo
  (`assets/tmdb-logo.svg`, provenance in `NOTICE`) and the exact notice: *"This
  product uses the TMDB API but is not endorsed or certified by TMDB."* The
  logo must be less prominent than Kinema's branding.
- **Limits:** connect timeout 10 s, total timeout 15 s, 2 MiB response cap,
  500 titles per page. TMDB redirects are disabled so the bearer token can never
  be forwarded.

## Indexers

- **One indexer is selected explicitly** (default: Torrentio). There is no
  federation, and no credentials are forwarded to indexers.
- Requests are keyed by IMDb ID. Parsed quality, size and seed counts are
  **unverified hints**, used for sorting, filtering and release matching
  (below). They never identify a file.
- **Per-episode file hints** (`behaviorHints.filename`, `fileIdx`) are
  explicit identity, applied only through `select_file_with_hint`'s guards:
  an exact basename match; an index only as a Real-Debrid file ID (AllDebrid
  IDs are synthetic and never used); a hint is refused when the file name has
  a contradicting or combined episode marker. **An index-only hint** (Peerflix
  sends no file name) is accepted only when the file's own full marker equals
  the target episode. *Why:* real captures showed Torrentio hints never chose
  a wrong file, while one Peerflix index named the wrong episode with nothing
  to cross-check it.
- **Episode markers** are one standalone `SxxExx` or `NxNN` token, with an
  optional `v<digit>` revision (`S01E05v2`). Not markers: `WxH` resolutions (a
  season side of 3+ digits or an episode side of 4+, e.g. `1448x1080`) and
  `s<digit>` inside a word (`FS88`). Combined episodes (`S01E01E02`,
  `S01E01-02`, a following bare number) are always refused.
- **Release matching** (`kinema_core::episode_continuity`) parses group,
  resolution, source, codec, HDR/DV and audio from the release and file names,
  and orders candidates by those traits, then size ratio, then seeders. A
  missing trait never counts as a match. Only a **strong** match (same group
  and same resolution) may be chosen without the user; anything else is a
  ranked list for the user. Choosing a torrent this way never relaxes file
  choice: the file inside is still picked by the rules here, or the list opens.
  Traits are always re-derived from stored raw names, never persisted.
- Rows that only have a direct URL, and duplicate hash/file-index rows, are
  omitted and counted. A successful empty result is valid. Invalid envelopes or
  hashes are **errors**, not empty results.

## Debrid

- Configured services report **account presence only**. A read-only listing
  shows which releases are already in the user's account. Real-Debrid's
  instant-availability endpoint is disabled upstream, and AllDebrid's
  `magnet/upload` has side effects, so Kinema does not claim "cached on debrid".
  See the [proposal](../proposals/debrid-availability.md).
- Preparation adds the magnet, waits for readiness (at most 30 polls, which
  check cancellation every 100 ms, with the provider's minimum intervals), lists
  files and unlocks the chosen link.
- **MUST: file choice is unambiguous.** The pure policy in `kinema-core::sources`
  picks the file, or the user chooses. Never pick the largest file as a guess,
  and never fall back silently to local torrenting. *Why:* a guessed file plays
  the wrong episode and saves progress against the wrong identity.
- **MUST: an upstream URL goes only to the media service.** It is never given to a
  player, logged or persisted. *Why:* it is a bearer capability for the user's
  debrid account.
- **Limits:** 20 s request timeout (8 s connect), no redirects, 4 MiB
  responses, 1,000 indexer rows, 4,096 files per provider catalog. URLs must be
  HTTPS.
- An account check means "the credential authenticates". It does not mean
  premium status or playback readiness.

## Subtitles (OpenSubtitles.com)

`providers::opensubtitles::OpenSubtitlesClient` is a blocking adapter for the REST
API, using the **user's own** `Api-Key` (nothing is bundled) and an optional user
login. Call it only from an owned background worker; it passes a `&AtomicBool` for
cancellation like the debrid adapters. Pure types, ranking, the OpenSubtitles
movie hash and `PlaybackTarget` conversion live in `kinema_core::subtitle_search`.

- **Headers:** `Api-Key`, `User-Agent: Kinema v<version>`, `Accept: application/json`,
  and `Authorization: Bearer` whenever a session exists (always on vip-api). Key and
  token are marked sensitive. The link host receives neither.
- **Hosts (HTTPS allowlist):** `api.opensubtitles.com`; `vip-api.opensubtitles.com`
  only when `/login` returned it as `base_url`; `www.opensubtitles.com` for
  download links. Anything else is `InvalidResponse`. No redirects: a 3xx is
  `ServiceFailure`.
- **Search** sends one canonical query (alphabetical parameters, lowercase, no
  default values, IMDb ids without `tt`/leading zeros, sorted comma-joined
  languages, `+` for spaces, 16-hex `moviehash`) and returns ranked results.
  `ai_translated`/`machine_translated` are omitted (service defaults); machine
  translations are also dropped locally. Only page 1 is read. Items with no
  usable file are skipped; a nonempty `data` where every item is malformed is
  `InvalidResponse`. An empty `data` is a successful empty result.
- **Download** is `POST /download` then a bounded GET of the temporary link
  (never logged, cached or returned). The file cap is
  `kinema_core::subtitles::MAX_SUBTITLE_BYTES`; empty bodies are rejected. JSON
  bodies are capped at 4 MiB.
- **Errors:** `Empty` is not an error. 401 on `/login` is `LoginRejected`; 401/403
  otherwise `ApiKeyRejected`; 401 on `/download` without a session `SignInRequired`
  (with a session, any 401 is `SessionExpired`); 406 from `/download` is
  `QuotaExhausted { reset_time_utc }`; 429 `RateLimited`; 5xx, other 4xx and 3xx
  `ServiceFailure`; transport `Network`; `Cancelled`; malformed `InvalidResponse`.
  The 406-as-quota and 403-as-bad-key mappings are assumptions from the public
  reference, not live-verified.
- **Limits:** requests are spaced 200 ms (5 per second), logins 1 s. After a 401 on
  `/login` the client remembers an in-memory fingerprint of those credentials and
  refuses to send them again without a request.
- **Logging:** only host, status, result/byte counts, durations and fixed
  categories. Never the key, token, credentials, query, file names or links.
- Tests use loopback fixtures; no live OpenSubtitles account, key or download has
  been exercised.

## Credentials

- Credentials are stored **only in the OS keyring** through the `keyring` crate,
  under service name `kinema`, one entry per account (`credentials.rs`).
  Linux needs a session D-Bus and an unlocked Secret Service. KDE is not
  required.
- **MUST: the service name and account names are a persisted contract.**
  Renaming either orphans every stored credential. The service was renamed
  from `kinema-next` once, before 1.0, with no copy step: users re-enter their
  tokens, and old `kinema-next` entries are left untouched. Legacy C++ app
  entries (KWallet folders) are never read or migrated.
- Credential states are distinct: present, **missing** (normal for optional
  lookups and not logged as a warning), **access denied** and **store
  unavailable**. None of these is ever treated as a cached success.
- In memory, secrets are wrapped in zeroizing types. Setting or deleting a
  credential bumps a process-wide generation, so cached clients are rebuilt. An
  authentication rejection also invalidates the cached client.
- The keyring library can cache a failed initialization for the rest of the
  process. After restoring access to the store, restart Kinema.
- **OpenSubtitles** uses three accounts: `opensubtitles-api-key` (the user's own
  key, nothing is bundled), and the optional `opensubtitles-username` and
  `opensubtitles-password`. `OpenSubtitlesCredentialStore::credentials()` returns
  the key (missing is distinct from access denied or unavailable) and a login
  only when both parts exist; a half-stored login is ignored, and a failed
  login save removes what it wrote. Edits bump `opensubtitles_generation()`.
  The login token is never stored. Presence is read on demand when Settings
  opens, not at startup. **Test** on the key row runs one fixed search; on the login row it
  signs in and reads the allowance. Neither spends a download.
- Credential presence in Settings means "stored", not "accepted by the service".
  **Test** uses the saved token, not unsaved text.

## Logging and privacy

`logging.rs` writes to stderr and to daily files (`kinema.YYYY-MM-DD`, 7 files
kept) through a bounded non-blocking writer. `RUST_LOG` overrides the default
filter.

Logging is **allowlisted**:

- **Allowed:** IMDb/TMDB IDs, durations, status codes, counts, sizes, fixed enum
  categories and session/player IDs.
- **MUST never be logged:** credentials, bearer tokens, debrid or signed URLs, magnets,
  full query strings, raw `reqwest` or native error text, or local file paths
  in user-facing errors.
- **Helpers:** use `logging::safe_url` (host only), `safe_url_with_path` (only
  for public paths with no tokens) and `http_error_kind`.
- **Levels:** `error` is for bugs and unrecoverable failures, `warn` for user-visible failed operations,
  `info` for lifecycle, `debug` for per-request outcomes and `trace` for
  noise.

## Change risk

- **High: keyring service and account names.** These are a persisted contract,
  and changing them strands users' stored credentials.
- **High: anything that touches secrets or logging.** Leaks are irreversible.
  Review every new log field against the allowlist.
- **Medium: adapters.** External APIs, terms and rate limits change. Test with
  loopback fixtures, and keep failures distinct from empty results.
- **Low: priority defaults and enrichment tuning.** These are easy to change.
