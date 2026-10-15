// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Debrid.h"

#include <QDate>
#include <QList>
#include <QMetaType>
#include <QString>
#include <QStringList>
#include <QUrl>

#include <optional>

namespace kinema::domain {

enum class MediaKind {
    Movie,
    Series,
};

inline QString mediaKindToPath(MediaKind k)
{
    return k == MediaKind::Movie ? QStringLiteral("movie") : QStringLiteral("series");
}

/// Lightweight summary returned by Cinemeta's search endpoint.
struct MetaSummary {
    QString imdbId;
    MediaKind kind = MediaKind::Movie;
    QString title;
    std::optional<int> year;
    /// Full release date (movie `released` / series first-air date) when
    /// Cinemeta exposes one. Consumed by the future-release UI so the
    /// app can badge unreleased titles and skip Torrentio for them.
    std::optional<QDate> released;
    QUrl poster;
    QString description;
    std::optional<double> imdbRating;
};

/// Full metadata returned by Cinemeta's meta endpoint.
struct MetaDetail {
    MetaSummary summary;
    QUrl background;
    QStringList genres;
    std::optional<int> runtimeMinutes;
    QStringList cast;
};

/// One stream row returned by Torrentio.
struct Stream {
    /// Raw first line of the `name` field, e.g. "Torrentio 1080p".
    QString qualityLabel;
    /// Parsed resolution tag (e.g. "1080p", "2160p", "720p", "—").
    QString resolution;
    /// First line of the `title` field — the release name.
    QString releaseName;
    /// Remaining lines of `title` joined with " · " for display.
    QString detailsText;

    std::optional<qint64> sizeBytes;
    std::optional<int> seeders;
    QString provider;

    /// Hex info hash, empty if the stream only exposes a direct URL.
    QString infoHash;
    /// Real-Debrid direct URL, empty unless RD is configured and produced it.
    QUrl directUrl;

    /// Index of the chosen file inside the torrent (`fileIdx` from
    /// Torrentio's `behaviorHints`). `-1` when Torrentio does not
    /// pin a file (single-file torrent or when the response leaves
    /// it to the client to decide).
    int fileIndex = -1;
    /// Hint at the chosen file name (`behaviorHints.filename`).
    /// Empty when Torrentio does not provide one.
    QString fileNameHint;
    /// Optional extra source trackers that Torrentio attached. Useful
    /// when re-adding the magnet against libtorrent so peer discovery
    /// is not gated on the stripped-down default tracker list.
    QStringList sources;

    /// Lower-cased ISO-ish language code as advertised by the indexer
    /// (e.g. `"en"`, `"es"`). Empty when the indexer doesn't surface
    /// a language field — Torrentio rows currently leave this blank;
    /// Peerflix populates it for every row. Consumed by
    /// `core::stream_filter::ClientFilters` to honour the
    /// "Non-English" hide-variant toggle.
    QString language;

    /// Which debrid provider the upstream indexer associated with
    /// this row, when it tagged one (`[RD+]`, `[AD download]`, or
    /// the equivalent Peerflix `providers` array entry).
    /// `DebridProvider::None` means the row is a plain torrent /
    /// direct URL with no debrid involvement — the common case
    /// when no provider is configured upstream.
    DebridProvider debridProvider = DebridProvider::None;

    /// True when the indexer signalled the row is already cached on
    /// the debrid account (instantly playable, e.g. Torrentio's
    /// `[RD+]` tag, or Peerflix surfacing the provider in its
    /// structured array). False when the indexer signalled the row
    /// exists in debrid but is not cached yet (e.g. `[RD download]`).
    /// Only meaningful when `debridProvider != DebridProvider::None`.
    bool debridCached = false;
};

/// One episode row from Cinemeta's series meta.videos[].
struct Episode {
    int season = 0; ///< 0 = specials
    int number = 0;
    QString title;
    QString description;
    QUrl thumbnail;
    std::optional<QDate> released;

    /// Torrentio stream id: "ttXXXXXX:S:E".
    QString streamId(const QString& imdbId) const
    {
        return QStringLiteral("%1:%2:%3").arg(imdbId).arg(season).arg(number);
    }
};

/// Full metadata returned by Cinemeta's meta endpoint for a series.
struct SeriesDetail {
    MetaDetail meta;
    /// All episodes across all seasons, sorted by (season, number).
    /// Season 0 (specials) is excluded from the season selector but kept here.
    QList<Episode> episodes;
};

} // namespace kinema::domain

Q_DECLARE_METATYPE(kinema::domain::MetaSummary)
Q_DECLARE_METATYPE(kinema::domain::MetaDetail)
Q_DECLARE_METATYPE(kinema::domain::Stream)
Q_DECLARE_METATYPE(kinema::domain::Episode)
Q_DECLARE_METATYPE(kinema::domain::SeriesDetail)
