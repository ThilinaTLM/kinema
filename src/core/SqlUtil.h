// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/PlaybackContext.h"

#include <QDateTime>
#include <QString>
#include <QTimeZone>

namespace kinema::core::sql {

inline constexpr const char* kIsoFmt = "yyyy-MM-ddTHH:mm:ssZ";

inline QString isoUtc(const QDateTime& dt)
{
    return dt.toUTC().toString(QString::fromLatin1(kIsoFmt));
}

inline QDateTime parseIsoUtc(const QString& s)
{
    auto dt = QDateTime::fromString(s, Qt::ISODate);
    if (!dt.isValid()) {
        dt = QDateTime::fromString(s, QString::fromLatin1(kIsoFmt));
    }
    if (dt.isValid()) {
        dt.setTimeZone(QTimeZone::utc());
    }
    return dt;
}

/// Qt's QSQLITE driver binds a null QString as SQL NULL, which clashes
/// with `NOT NULL DEFAULT ''` columns. Normalise first.
inline QString nullSafe(const QString& s)
{
    return s.isNull() ? QString::fromLatin1("") : s;
}

/// Append `AND season = ?` / `AND season IS NULL` (and similarly for
/// episode) to a SELECT WHERE clause. Caller binds via
/// `bindKeyFilter()` in the same order.
inline void appendKeyFilter(QString& sql, const api::PlaybackKey& key)
{
    sql += key.season.has_value()
        ? QStringLiteral(" AND season = ?")
        : QStringLiteral(" AND season IS NULL");
    sql += key.episode.has_value()
        ? QStringLiteral(" AND episode = ?")
        : QStringLiteral(" AND episode IS NULL");
}

template <typename Q>
inline void bindKeyFilter(Q& q, const api::PlaybackKey& key)
{
    if (key.season.has_value()) {
        q.addBindValue(*key.season);
    }
    if (key.episode.has_value()) {
        q.addBindValue(*key.episode);
    }
}

} // namespace kinema::core::sql
