# Configuring Kinema

Kinema connects several independent services to turn a title into something you
can watch:

1. **TMDB** supplies the catalogue, artwork, metadata, search results, and
   recommendations.
2. An **indexer** finds candidate streams for the selected movie or episode.
3. Kinema either transfers the selected torrent with its built-in BitTorrent
   backend or asks an optional **debrid provider** for an HTTP stream.
4. The **player** opens that stream.

TMDB is required for the normal discovery and browsing experience. The public
indexers work without accounts. A debrid subscription is optional because
Kinema also has a built-in torrent backend, but debrid is generally the
simpler and faster option when a release is already cached by the provider.

> [!NOTE]
> Indexers and debrid providers do different jobs. An indexer finds releases;
> a debrid provider retrieves or streams the release you select. Configuring a
> debrid account does not replace the indexer.

## Recommended first-run setup

| Step | Settings page | Recommendation |
| --- | --- | --- |
| 1 | **TMDB** | Add and test your own TMDB v4 read access token. |
| 2 | **Debrid** | Add a Real-Debrid or AllDebrid credential and select it, or select **None** to use public BitTorrent peers. |
| 3 | **Indexers** | Keep Torrentio initially; test it and use Peerflix as an alternative if needed. |
| 4 | **Player** | Select **Embedded (built-in mpv)** when available. Otherwise install and select external mpv or VLC. |

Open the application menu and select **Settings** to access these pages.

## 1. TMDB

[The Movie Database (TMDB)](https://www.themoviedb.org/) powers Discover,
Browse, title search, movie and series details, episode metadata, artwork, and
recommendations. It does not provide video streams.

Without an effective TMDB token, these catalogue features cannot load. A
packager may compile a shared token into Kinema, but upstream builds do not
assume one is present. Your saved token takes priority over any token bundled
by a packager.

### Create and save a token

1. Create or sign in to a TMDB account.
2. Open <https://www.themoviedb.org/settings/api> and request API access if the
   account does not already have it.
3. Copy the **API Read Access Token** from the v4 authentication section. Use
   the long v4 read token, not the shorter v3 API key.
4. In Kinema, open **Settings → TMDB**.
5. Paste it into **v4 Read Access Token**.
6. Select **Test connection**. After the test succeeds, select **Save token**.

The token is stored through the system keyring, such as KWallet or
GNOME Keyring, rather than as plaintext in `kinemarc`. **Remove token** deletes
the user token and returns to the build's bundled token, if it has one.

## 2. Debrid provider

A debrid provider accepts a magnet/torrent selected in Kinema and exposes its
content over HTTP. If the provider already has the release cached, playback
can begin without waiting for local torrent peers. An uncached release may
still need to be fetched by the provider first. Availability and speed depend
on the provider, account, release, and region.

Kinema supports:

- [Real-Debrid](https://real-debrid.com/)
- [AllDebrid](https://alldebrid.com/)

These are third-party services and normally require their own paid accounts.
Kinema does not create or include a subscription.

### Configure Real-Debrid

1. Sign in to Real-Debrid and copy the token from
   <https://real-debrid.com/apitoken>.
2. Open **Settings → Debrid** in Kinema.
3. Paste it into the Real-Debrid **API token** field.
4. Select **Test connection**, then **Save token**.
5. Under **Active provider**, select **Real-Debrid**.

### Configure AllDebrid

1. Sign in to AllDebrid and create or copy a key at
   <https://alldebrid.com/apikeys/>.
2. Open **Settings → Debrid** in Kinema.
3. Paste it into the AllDebrid **API key** field.
4. Select **Test connection**, then **Save API key**.
5. Under **Active provider**, select **AllDebrid**.

Both credentials can remain saved while only one provider is active. Changing
the active provider affects new transfers; existing download rows can remain
associated with the provider that created them. Credentials are kept in the
system keyring.

Select **None** to use Kinema's built-in libtorrent backend instead. This needs
no third-party account, but it connects to public BitTorrent peers, exposes
your IP address to those peers, and depends on peer availability. Saved debrid
credentials remain in the keyring when **None** is selected.

## 3. Stream indexer

An indexer receives the selected movie or episode identifier and returns
candidate releases, such as magnet links. Indexers do not supply TMDB metadata
and do not perform debrid transfers.

Open **Settings → Indexers**, select one active indexer, and use **Test
connection**. Both included public defaults are zero-configuration and require
no account.

### Torrentio

[Torrentio](https://torrentio.strem.fun/) is the default. Its public Stremio
addon aggregates releases from multiple sources.

- **Default sort** controls the sort encoded into Torrentio requests:
  **Seeders**, **Size**, or **Quality & Size**. The results can still be
  re-sorted from the stream-list header.
- **Base URL** defaults to `https://torrentio.strem.fun`. Leave it unchanged
  for the public service. Change it only when using a trusted compatible host
  or self-hosted deployment.
- **Reset to public host** restores the default URL.

### Peerflix

Peerflix is an alternative public Stremio-compatible addon with a different
scraper pool. It can find releases absent from Torrentio, and many of its
results are Spanish-language.

- **Base URL** defaults to `https://peerflix.mov`.
- Leave it unchanged unless using a trusted compatible deployment.
- To reduce non-English results, enable **Settings → Streams → Hide variants
  → Non-English**.

Only the active indexer is queried. If a title has no useful results, switch to
the other indexer and try again. Public services can be unavailable or change
independently of Kinema. Treat custom hosts as trusted services: they receive
the media identifiers in your queries.

## 4. Player

Open **Settings → Player** and choose how Kinema launches a selected stream.

### Embedded player (recommended)

**Embedded (built-in mpv)** plays inside Kinema and provides the integrated
player chrome, resume handling, episode navigation, track selection, and other
in-app controls. Official Kinema packages are built with this support; the
AppImage and portable archive bundle the required libraries.

The embedded option requires both **libmpv 0.36 or newer** and **mpvqt at build
time**. If the setting says **Embedded (not built with libmpv)**, installing a
runtime library alone does not add the feature to that existing binary. Use an
official build or install the development packages and rebuild Kinema:

```bash
cmake -B build -S . -DCMAKE_BUILD_TYPE=RelWithDebInfo \
    -DKINEMA_ENABLE_MPV_EMBED=ON
cmake --build build -j"$(nproc)"
```

Common development package names are `libmpv-dev` and `libmpvqt6-dev` (or
`libmpvqt-dev`) on Debian-family systems, and `mpv-libs`/`mpv` plus `mpvqt` on
Fedora, Arch, and openSUSE. Names vary by distribution.

Embedded-player options are:

- **Use hardware decoding** — recommended; disable it only to diagnose GPU or
  video-driver problems. Software decoding uses more CPU.
- **Preferred audio language** — comma-separated language codes, for example
  `en,ja`.
- **Preferred subtitle language** — language codes such as `en`.
- **Offer skip intro / credits** — offers actions when suitable chapters are
  present in series media.
- **Resume prompt threshold** — controls how many saved seconds are required
  before Kinema offers to resume playback. Set it to `0` to allow a prompt for
  any saved progress.

### External mpv or VLC

Choose **mpv** or **VLC** to open streams in a separate process. Kinema marks an
option as not found when its executable is absent from `PATH`; install the
player with the distribution package manager and reopen Settings. External
players do not provide all embedded-player integration.

### Custom command

Choose **Custom command** to use another URL-capable player. Put `{url}` where
the stream URL should be inserted:

```text
mpv --fs --really-quiet {url}
```

If `{url}` is omitted, Kinema appends the URL as the final argument. The field
is tokenized like a command line, but it is not a general shell script; prefer
a wrapper executable for pipelines, redirection, or complex shell logic.

## Verify the setup

1. Open **Discover** and confirm that catalogue rows and artwork load.
2. Open a movie or episode and request streams; confirm that the active
   indexer returns results.
3. Select a stream. For debrid playback, check that the selected provider
   accepts or resolves it. For **None**, allow time for torrent peers and the
   startup buffer.
4. Confirm that playback starts in the selected player.

A successful connection test checks one integration only. For example, a
working TMDB test does not test the indexer or debrid provider.

## Troubleshooting

- **Discover or Browse is empty:** retest the TMDB token and make sure it is a
  v4 read access token. Remove and save it again if the keyring was locked
  during setup.
- **No stream results:** test the active indexer, reset its public URL, try the
  other indexer, and review filters under **Settings → Streams**.
- **Debrid authorization fails:** generate a fresh credential on the
  provider's site, save it, and ensure that the matching provider is active.
- **A debrid stream waits:** it may not be cached and the provider may still be
  downloading it. Try a better-seeded or cached release.
- **Embedded player is unavailable:** see the build-time requirements above,
  or select an external player.
- **External player is not found:** verify `mpv --version` or `vlc --version`
  works from the same desktop environment that starts Kinema.

Runtime logs are written to
`~/.local/share/kinema/logs/kinema.log`. See the main
[README](../README.md#troubleshooting) for logging categories and debug
options.

## Security and legal notes

Never post TMDB or debrid credentials in an issue, screenshot, or log. Remove
and rotate a credential immediately if it is exposed. Only enter credentials
in Kinema itself or on the provider's HTTPS website.

Kinema is a client for third-party metadata, indexer, debrid, and playback
services. You are responsible for those services' terms and for complying with
the laws of your jurisdiction regarding content you retrieve, stream, or
download.
