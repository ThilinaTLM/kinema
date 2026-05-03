// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

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

/// Number of days before the official release date at which we begin
/// attempting to fetch streams. Torrents commonly appear roughly a
/// day before the official date, so a 1-day grace window catches them
/// without flooding providers with pointless lookups.
inline constexpr int kStreamLookaheadDays = 1;

/**
 * True iff `d` is far enough in the future that stream providers
 * almost certainly have nothing yet — i.e. strictly more than
 * `kStreamLookaheadDays` after today. Used only to gate Torrentio
 * requests; UI badging continues to use `isFutureRelease()` so an
 * item stays visually "Upcoming" until its actual release date. A
 * missing or invalid date returns false (allow the fetch).
 */
bool isReleaseTooEarlyForStreams(const std::optional<QDate>& d);

} // namespace kinema::core
