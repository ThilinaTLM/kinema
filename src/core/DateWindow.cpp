// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/DateWindow.h"

namespace kinema::core {

DateRange dateRangeFor(DateWindow w, QDate today)
{
    DateRange r;
    switch (w) {
    case DateWindow::PastMonth:
        r.gte = today.addMonths(-1);
        r.lte = today;
        break;
    case DateWindow::Past3Months:
        r.gte = today.addMonths(-3);
        r.lte = today;
        break;
    case DateWindow::ThisYear:
        r.gte = QDate(today.year(), 1, 1);
        r.lte = today;
        break;
    case DateWindow::Past3Years:
        r.gte = today.addYears(-3);
        r.lte = today;
        break;
    case DateWindow::Any:
        break;
    }
    return r;
}

QString dateWindowToString(DateWindow w)
{
    switch (w) {
    case DateWindow::PastMonth:
        return QStringLiteral("month1");
    case DateWindow::Past3Months:
        return QStringLiteral("month3");
    case DateWindow::ThisYear:
        return QStringLiteral("year");
    case DateWindow::Past3Years:
        return QStringLiteral("year3");
    case DateWindow::Any:
        return QStringLiteral("any");
    }
    return QStringLiteral("year");
}

DateWindow dateWindowFromString(const QString& s, DateWindow fallback)
{
    if (s == QLatin1String("month1")) {
        return DateWindow::PastMonth;
    }
    if (s == QLatin1String("month3")) {
        return DateWindow::Past3Months;
    }
    if (s == QLatin1String("year")) {
        return DateWindow::ThisYear;
    }
    if (s == QLatin1String("year3")) {
        return DateWindow::Past3Years;
    }
    if (s == QLatin1String("any")) {
        return DateWindow::Any;
    }
    return fallback;
}

} // namespace kinema::core
