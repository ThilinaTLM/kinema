// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"

#include <QDate>
#include <QDateTime>
#include <QMetaType>
#include <QString>
#include <QUrl>

#include <optional>

namespace kinema::api {

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

} // namespace kinema::api

Q_DECLARE_METATYPE(kinema::api::LibraryTitle)
Q_DECLARE_METATYPE(kinema::api::LibraryEpisode)
