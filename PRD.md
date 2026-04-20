# Kinema — Product Requirements Document

**Status:** Draft
**Product name:** Kinema
**Application id:** `dev.tlmtech.kinema`
**Owner:** Thilina Lakshan
**Last updated:** 2026-04-20

---

## 0. Name & Branding

**Kinema** takes its name from the Greek *κίνημα* (*kínēma*, "movement") —
the etymological root of the word *cinema*. The name is intentionally
backend-agnostic: it describes what the app is *for* (watching films),
not the specific service it pulls from. It also fits naturally alongside
other KDE applications with the familiar K-prefix (Kate, Krita, Konsole,
Kdenlive, Kaffeine).

- **Product name:** Kinema
- **Binary / command:** `kinema`
- **Reverse-DNS application id:** `dev.tlmtech.kinema`
  (used for the `.desktop` file, D-Bus service name, Flatpak id, and
  `QCoreApplication` organization/application identifiers)
- **Tagline (working):** *Cinema in motion.*

## 1. Overview

Kinema is a native Linux desktop application that lets users search for
movies and TV series, browse available torrent sources via
[Torrentio](https://torrentio.strem.fun/), and either grab a magnet link or —
for Real-Debrid subscribers — stream the file directly in a local video
player (mpv or VLC).

It is a graphical successor to the existing `torrentio-cli` tool: same data
sources, but with metadata-rich browsing (posters, titles, descriptions) and
one-click playback, without requiring Stremio.

## 2. Goals

- Let a user go from **"I want to watch X"** to **playing it in mpv/VLC** in
  as few clicks as possible.
- Surface the **same metadata quality** a user would expect from Stremio
  (poster, title, year, description) before picking a torrent.
- Expose Torrentio's full range of results (quality, size, seeders, provider)
  so the user can make an informed choice.
- Support **both free users** (magnet / `.torrent` output) and
  **Real-Debrid users** (direct streaming URLs).
- Feel like a first-class **KDE Plasma** citizen: native look, keyboard
  shortcuts, system notifications, secure credential storage.
- Establish **Kinema** as a recognizable, backend-agnostic brand that can
  grow beyond Torrentio if other sources are added later.

## 3. Non-Goals

- Not a Stremio replacement — no add-on ecosystem, no library management,
  no watch history sync, no subtitles management (initially).
- Not a torrent client — downloading/seeding is delegated to the user's
  existing torrent client or to Real-Debrid.
- Not a media library / catalog app — search-driven only, no "my collection"
  view in v1.
- No Windows/macOS support in v1 (code should stay portable, but only Linux
  is tested and packaged).
- No built-in video player — playback is delegated to mpv or VLC.

## 4. Target Users

- **Linux / KDE users** comfortable with the idea of magnet links and
  external players.
- **Real-Debrid subscribers** who want Stremio-style instant streaming
  without running Stremio.
- **Power users** who currently use `torrentio-cli` and want a friendlier
  interface for browsing posters and picking episodes.

## 5. User Stories

1. *As a user,* I want to **search "The Matrix"** and see matching movies
   with posters so I can confirm I picked the right one.
2. *As a user,* I want to **search "Breaking Bad"**, pick a season, then pick
   an episode, and see torrents for that exact episode.
3. *As a user,* I want to **see quality, size, and seeder count** for each
   torrent so I can pick the best one.
4. *As a user,* I want to **copy a magnet link** to paste into my torrent
   client.
5. *As a user,* I want to **save a `.torrent` file** to disk.
6. *As a Real-Debrid user,* I want to **click "Play"** and have the stream
   open instantly in **mpv** (or VLC).
7. *As a Real-Debrid user,* I want to **filter to only cached results** so
   every result is instantly playable.
8. *As a user,* I want my **Real-Debrid token stored securely** (not in a
   plaintext config file).

## 6. Functional Requirements

### 6.1 Search movies and TV series with metadata

- A search bar accepts a free-text query (title) **or** an IMDB id
  (e.g. `tt0133093`).
- A toggle / mode selector distinguishes **Movie** vs **TV Series** search.
- Results are shown as a grid (or list) of cards with:
  - Poster image
  - Title
  - Year
  - Type (movie / series)
  - Short description / plot (when available)
  - IMDB rating (when available)
- Clicking a result opens its **detail view**:
  - Larger poster, full description, genres, runtime, cast (best-effort from
    Cinemeta).
  - For series: a **season selector** and **episode list** (episode number,
    title, air date, thumbnail when available).
- Empty-state and error-state messaging when nothing is found or the network
  fails.

### 6.2 Fetch torrent options via Torrentio

- From a movie detail view or a specific episode, the user triggers
  **"Find torrents"** (or it auto-loads).
- Results are displayed in a sortable table/list with per-row:
  - Quality label (e.g. 4K, 1080p, 720p)
  - Release name
  - File size
  - Seeder count (when available)
  - Provider / tracker source
  - Indicator for Real-Debrid status:
    - `[RD+]` cached → instantly streamable
    - `[RD download]` uncached → will be fetched by RD on click
    - none → plain magnet only
- Filters available in the UI:
  - Quality (multi-select: 2160p / 1080p / 720p / 480p / other)
  - Providers (multi-select)
  - Sort: seeders / size / quality+size
  - **Cached on Real-Debrid only** (when RD is configured)
  - Result limit
- Filter defaults are configurable in Settings.

### 6.3 Obtain magnet link, `.torrent` file, or Real-Debrid direct link

For any selected result, the user can:

- **Copy magnet link** to clipboard.
- **Open magnet link** with the system's default torrent handler
  (`xdg-open` / KDE's URL handler).
- **Save as `.torrent`** — generate or download a `.torrent` file to a
  user-chosen location (best-effort; magnet is always available).
- **Copy direct URL** (Real-Debrid streams only).
- **Open direct URL in browser** (Real-Debrid streams only).

The app automatically appends a set of public trackers to magnet links so
they work out of the box.

### 6.4 Play directly in mpv or VLC

- For any result that has a **playable URL** (Real-Debrid direct link), a
  **"Play"** action is available.
- For magnet-only results, a **"Stream"** action may be offered if a
  streaming-capable client is detected (e.g. VLC with magnet support, or
  `peerflix`); otherwise the action is hidden/disabled with a tooltip.
- The user's **preferred player** is configurable in Settings:
  - mpv (default when available)
  - VLC
  - Custom command (advanced users)
- Launching a player:
  - Spawns the player as a detached process with the URL as an argument.
  - Does not block the UI; the app keeps running.
  - Surfaces a system notification on launch and on failure.
- Player availability is auto-detected at startup; missing players are
  greyed out with a hint on how to install them.

### 6.5 Real-Debrid integration

- Optional. The app is fully usable without an RD account.
- User provides their RD API token once, via **Settings**.
- Token is stored in **KWallet** (or the freedesktop Secret Service), never
  in plaintext config.
- When a token is present:
  - Torrentio is queried with the RD config so cached results return direct
    URLs.
  - The UI displays cache status badges on results.
  - The "Cached only" filter becomes available.
- The user can clear the token from Settings at any time.

### 6.6 Settings

A settings dialog exposes:

- Real-Debrid token (with "Test connection" button).
- Default player (mpv / VLC / custom command).
- Default search filters (quality, providers, sort, limit).
- Default download location for `.torrent` files.
- Network timeout.
- Clear cache / credentials.

### 6.7 Tech stack

- **Language:** C++ (C++20 or newer).
- **UI toolkit:** Qt 6 (Widgets).
- **Framework:** KDE Frameworks 6 — at minimum for i18n, config, secure
  credential storage, notifications, and URL handling.
- **Build system:** CMake with extra-cmake-modules (ECM).
- **Application identity:**
  - Application id: `dev.tlmtech.kinema`
  - Desktop entry: `dev.tlmtech.kinema.desktop`
  - Executable: `kinema`
  - Qt `QCoreApplication::setOrganizationDomain("tlmtech.dev")`,
    `setApplicationName("Kinema")`
- **External services:**
  - Cinemeta (`v3-cinemeta.strem.io`) — metadata + IMDB id resolution.
  - Torrentio (`torrentio.strem.fun`) — torrent streams.
  - Real-Debrid API — cached streams & direct URLs (optional).

## 7. Non-Functional Requirements

- **Performance:** search results visible within ~2 s on a typical
  broadband connection; torrent list within ~3 s. UI never blocks on
  network calls.
- **Reliability:** network errors, empty results, and malformed upstream
  responses surface clear, non-fatal error messages.
- **Security:** credentials stored only in the system keyring; no secrets
  logged; HTTPS for all upstream calls.
- **Accessibility:** full keyboard navigation, visible focus states,
  respects the system's Plasma theme (light / dark / high-contrast).
- **Localization-ready:** all user-facing strings wrapped for translation
  from day one (English is the only shipped locale initially).
- **Privacy:** no telemetry, no analytics. The app talks only to the three
  services listed above.
- **Footprint:** cold start under 1 s on modern hardware; memory under
  ~150 MB with a typical result set.

## 8. Primary User Flows

### 8.1 Watch a movie via Real-Debrid

1. User opens the app.
2. Types "Dune" in the search bar, keeps the default **Movie** mode.
3. Clicks the correct result card (*Dune: Part Two*).
4. Detail view loads; user clicks **"Find torrents"** (or it auto-loads).
5. Filters to **1080p** + **Cached only**.
6. Clicks **Play** on the top result → mpv opens and begins streaming.

### 8.2 Grab a magnet for a specific episode

1. User searches "Breaking Bad", switches to **Series** mode.
2. Opens the result, picks **Season 1**, picks **Episode 1**.
3. Torrent list loads for `tt...:1:1`.
4. User right-clicks the desired row → **Copy magnet link**.
5. Pastes it into their preferred torrent client.

### 8.3 First-time Real-Debrid setup

1. User opens **Settings → Real-Debrid**.
2. Pastes their API token, clicks **Test connection** → green check.
3. Clicks **Save**. Token is persisted to KWallet.
4. Subsequent searches show `[RD+]` / `[RD download]` badges and enable the
   "Cached only" filter.

## 9. Out of Scope (v1)

- Subtitles (download, selection, sync).
- Watch history, resume position, "continue watching".
- User accounts or multi-profile support.
- Bundled media player.
- Windows and macOS packaging.
- Other debrid providers (AllDebrid, Premiumize, TorBox).
- Stremio add-on protocol support beyond the Torrentio + Cinemeta endpoints
  already used.
- Mobile / touch / convergent UI.

## 10. Success Criteria

- A new user can install Kinema, search for a movie, and start playback in
  mpv via Real-Debrid in **under 2 minutes** end-to-end.
- A user without Real-Debrid can search, pick a result, and copy a working
  magnet link in **under 30 seconds**.
- Zero plaintext storage of the Real-Debrid token on disk.
- Kinema runs cleanly on a stock **KDE Plasma 6** desktop with no manual
  dependency tweaks beyond installing mpv or VLC.

## 11. Open Questions

- Should `.torrent` file export be a hard requirement, or is magnet-only
  acceptable for v1? (Torrentio returns info hashes, not `.torrent` blobs.)
- Should the app offer a "stream magnet via `peerflix`/`webtorrent`" option
  for non-RD users, or leave that to the torrent client?
- Flatpak first, or distro-native package (`.deb` / Arch AUR) first?
- Is a KDE-only UX acceptable, or should the app degrade gracefully on
  GNOME/other DEs (fall back from KWallet to libsecret, etc.)?

---

*This PRD focuses on product behavior. Implementation details (class
structure, specific KDE Frameworks, CMake layout) will live in a separate
technical design document.*
