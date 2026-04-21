// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QDate>
#include <QString>

#include <optional>

namespace kinema::core {

/**
 * Relative date windows offered by the Browse filter bar. Each value
 * maps to an inclusive [gte, lte] QDate range via `dateRangeFor()`.
 * The `lte` side is clamped to "today" on every non-Any window so
 * unreleased titles can't muddy rating-sorted results.
 */
enum class DateWindow {
    PastMonth,    ///< today.addMonths(-1) .. today
    Past3Months,  ///< today.addMonths(-3) .. today
    ThisYear,     ///< QDate(today.year(), 1, 1) .. today
    Past3Years,   ///< today.addYears(-3) .. today
    Any,          ///< no bounds
};

struct DateRange {
    std::optional<QDate> gte;
    std::optional<QDate> lte;
};

/// Snapshot-stable date-range resolution. `today` is parameterised so
/// unit tests can pin a date and assert exact boundary values.
DateRange dateRangeFor(DateWindow w,
    QDate today = QDate::currentDate());

/// Canonical serialisation token for KConfig. Stable across releases.
QString dateWindowToString(DateWindow w);

/// Parse a canonical token; unknown input returns `fallback` (defaults
/// to `ThisYear`, matching the Browse-page default).
DateWindow dateWindowFromString(const QString& s,
    DateWindow fallback = DateWindow::ThisYear);

} // namespace kinema::core
