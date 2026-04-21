// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include <QDate>
#include <QDateTime>
#include <QString>

#include <optional>

namespace kinema::core {

/**
 * Canonical "release date" formatter.
 *
 * Produces `d MMM yyyy` via `QLocale::system()`, e.g. `17 Jun 2019`
 * (month name localized). The day-month-year ordering is deliberate:
 * it's unambiguous across locales (no `6/17/19` vs `17/6/19`
 * confusion) and short enough for dense UI like the episode list.
 *
 * All UI surfaces that render a full QDate should go through this
 * helper so the format stays consistent.
 */
QString formatReleaseDate(const QDate& d);

/// Same as the QDate overload, applied to the local-time date portion.
QString formatReleaseDate(const QDateTime& dt);

/**
 * True iff `d` holds a value strictly after today. Titles releasing
 * today are treated as available (streams *may* be up by then, and
 * there's no useful badge to show). A missing date is never "future".
 */
bool isFutureRelease(const std::optional<QDate>& d);

} // namespace kinema::core
