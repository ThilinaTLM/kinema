// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/DateFormat.h"

#include <QLatin1String>
#include <QLocale>

namespace kinema::core {

QString formatReleaseDate(const QDate& d)
{
    if (!d.isValid()) {
        return {};
    }
    // Use QLocale so the month abbreviation is localized
    // ("Jun" / "juin" / "Juni" / ...). The day-month-year ordering is
    // fixed regardless of locale to avoid the DMY/MDY ambiguity of
    // QLocale::ShortFormat.
    return QLocale::system().toString(d, QStringLiteral("d MMM yyyy"));
}

QString formatReleaseDate(const QDateTime& dt)
{
    return formatReleaseDate(dt.date());
}

bool isFutureRelease(const std::optional<QDate>& d)
{
    return d.has_value() && d->isValid() && *d > QDate::currentDate();
}

} // namespace kinema::core
