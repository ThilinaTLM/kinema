// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Media.h"

#include <QDate>
#include <QDateTime>
#include <QMetaType>
#include <QString>
#include <QStringList>
#include <QUrl>

#include <optional>

namespace kinema::domain {

struct LibraryTitle {
    MediaKind kind = MediaKind::Movie;
    QString imdbId;
    int tmdbId = 0;
    QString title;
    std::optional<int> year;
    QUrl poster;
    QUrl backdrop;
    QString overview;
    std::optional<QDate> releaseDate;
    QDateTime addedAt;
    QDateTime updatedAt;
    /// Cinemeta `genres` array. Empty for rows saved before schema
    /// v7 until LibraryController's lazy backfill repopulates them.
    QStringList genres;
    /// Cinemeta `imdbRating` (0.0..10.0). std::nullopt when unknown.
    std::optional<double> imdbRating;
    /// Cinemeta `runtime` minutes. std::nullopt when unknown.
    std::optional<int> runtimeMinutes;
    /// Cinemeta `cast` array (top-billed). Empty until backfilled.
    QStringList cast;
};

struct LibraryEpisode {
    QString seriesImdbId;
    int season = 0;
    int episode = 0;
    QString title;
    QString overview;
    QUrl thumbnail;
    std::optional<QDate> releaseDate;
    QDateTime updatedAt;
};

} // namespace kinema::domain

Q_DECLARE_METATYPE(kinema::domain::LibraryTitle)
Q_DECLARE_METATYPE(kinema::domain::LibraryEpisode)
