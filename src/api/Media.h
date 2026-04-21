// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QDate>
#include <QList>
#include <QMetaType>
#include <QString>
#include <QStringList>
#include <QUrl>

#include <optional>

namespace kinema::api {

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

    bool rdCached = false; ///< `name` contains "[RD+]"
    bool rdDownload = false; ///< `name` contains "[RD download]"
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

} // namespace kinema::api

Q_DECLARE_METATYPE(kinema::api::MetaSummary)
Q_DECLARE_METATYPE(kinema::api::MetaDetail)
Q_DECLARE_METATYPE(kinema::api::Stream)
Q_DECLARE_METATYPE(kinema::api::Episode)
Q_DECLARE_METATYPE(kinema::api::SeriesDetail)
