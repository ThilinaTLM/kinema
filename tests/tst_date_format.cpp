// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/DateFormat.h"

#include <QDate>
#include <QLocale>
#include <QTest>

using namespace kinema::core;

class DateFormatTest : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void initTestCase()
    {
        // Pin the locale so month abbreviations are deterministic on
        // any runner. The C locale follows English conventions for
        // QLocale::toString formatting strings, giving us "Jun".
        QLocale::setDefault(QLocale::c());
    }

    void formatReleaseDate_formatsKnownDate()
    {
        QCOMPARE(formatReleaseDate(QDate(2019, 6, 17)),
            QStringLiteral("17 Jun 2019"));
    }

    void formatReleaseDate_handlesDateTime()
    {
        QCOMPARE(formatReleaseDate(QDateTime(QDate(2023, 1, 5),
                     QTime(12, 30))),
            QStringLiteral("5 Jan 2023"));
    }

    void formatReleaseDate_invalidReturnsEmpty()
    {
        QCOMPARE(formatReleaseDate(QDate()), QString());
    }

    void isFutureRelease_nulloptIsFalse()
    {
        QVERIFY(!isFutureRelease(std::nullopt));
    }

    void isFutureRelease_todayIsFalse()
    {
        const auto today = QDate::currentDate();
        QVERIFY(!isFutureRelease(std::optional<QDate> { today }));
    }

    void isFutureRelease_pastIsFalse()
    {
        QVERIFY(!isFutureRelease(std::optional<QDate> {
            QDate::currentDate().addDays(-1) }));
    }

    void isFutureRelease_tomorrowIsTrue()
    {
        QVERIFY(isFutureRelease(std::optional<QDate> {
            QDate::currentDate().addDays(1) }));
    }

    void isFutureRelease_invalidDateIsFalse()
    {
        QVERIFY(!isFutureRelease(std::optional<QDate> { QDate() }));
    }
};

QTEST_APPLESS_MAIN(DateFormatTest)
#include "tst_date_format.moc"
