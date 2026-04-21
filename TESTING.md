# Manual smoke tests

Run after a clean build. Every step should pass without a crash or
unlocalised user-facing string.

## M1 — movies & magnets

1. `./build/bin/kinema` — window titled *Kinema* opens.
2. Search `The Matrix` → poster grid with matches appears within ~2 s.
3. Click the top card → detail pane populates; torrents table appears
   within ~3 s.
4. Right-click a torrent → **Copy magnet link** → paste into a torrent
   client (e.g. qBittorrent) → the magnet is accepted and starts resolving.
5. Right-click a torrent → **Open magnet link** → the system's default
   handler launches.
6. Type `tt0133093` directly (IMDB-id shortcut) → the Matrix detail
   loads without a search step.
7. Disconnect network, search anything → a clean error state appears
   in the results pane; no crash, no frozen UI.
8. Close and reopen the app, repeat step 2 — poster grid populates
   near-instantly because posters are cached on disk.

## M2 — series & Real-Debrid

 9. Switch the search toggle to **TV Series**, search `Breaking Bad`.
    Poster grid appears within ~2 s.
10. Click the top card. The window switches into a **series focus
    view**: compact header at the top (back button, small poster,
    metadata), then a vertical split of episodes above and torrents
    below. S1E1 *Pilot* is auto-selected and its torrents load within
    ~3 s.
11. At least **4 episode rows** are visible without scrolling on a
    1100 px-tall window. The torrents table spans the full window
    width, with the Release column comfortably wide.
12. Drag the splitter between episodes and torrents; the ratio
    survives an app restart.
13. Pick **Season 2** from the dropdown. Episode list refreshes with
    thumbnails; clicking **E1** loads torrents for `tt0903747:2:1`.
14. Press **Esc** (or click **← Back to results** or **Alt+Left**).
    The grid reappears with the original scroll position and
    selection; the window title returns to *Kinema*.
15. Open **Tools → Real-Debrid…**, paste a real token, click **Test
    connection**. Green status line with your username.
16. **Save**. Run a movie search. Popular titles show `[RD+]` badges
    on at least a few rows.
17. Tick **Cached on Real-Debrid only**. Only `[RD+]` rows remain
    visible; un-ticking restores the full list. No refetch happens.
18. Right-click a cached row → **Copy direct URL** → paste into a
    browser: the file begins to stream from `*.real-debrid.com`.
19. Close + reopen the app. The SearchBar mode is remembered, badges
    still show, the cached-only checkbox state is preserved, and the
    focus-view splitter sits where you left it.
20. Open the dialog, click **Remove token**. Badges disappear, the
    checkbox hides; app reverts to pure magnet-mode.
21. Verify no plaintext token on disk:
    ```bash
    grep -rI "<first 10 chars of your token>" ~/.config ~/.local ~/.cache 2>/dev/null
    ```
    should yield nothing.

## M3 — mpv / VLC playback

22. With Real-Debrid still configured, search for any popular movie
    that has `[RD+]` cached rows. Right-click a cached row → **Play**.
    mpv opens and begins streaming within ~2 s; a desktop
    notification titled *Playing in mpv* appears.
23. **Double-click** a cached row — same behaviour, no menu step.
    Double-clicking an uncached row does nothing (tooltip on the
    disabled Play menu item explains why).
24. Close mpv. The Kinema window is unaffected — no dangling
    child process (`pgrep mpv` returns nothing).
25. Temporarily rename `mpv` out of `$PATH` (e.g.
    `sudo mv /usr/bin/mpv /usr/bin/mpv.bak`) and retry **Play**.
    Kinema falls back to **VLC** if present, otherwise shows a
    *Could not start mpv* notification and a status-bar line
    suggesting to install mpv. Restore mpv afterwards.
26. On a row without a direct URL (plain magnet), the **Play**
    menu item is greyed out and the context menu still offers
    **Copy magnet link** / **Open magnet link**.
