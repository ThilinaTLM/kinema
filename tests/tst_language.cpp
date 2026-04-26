// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/Language.h"

#include <QTest>

using namespace kinema::core::language;

class TstLanguage : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void testKnownCodes()
    {
        QCOMPARE(displayName(QStringLiteral("eng")), QStringLiteral("English"));
        QCOMPARE(displayName(QStringLiteral("ENG")), QStringLiteral("English"));
        QCOMPARE(displayName(QStringLiteral("spa")), QStringLiteral("Spanish"));
        QCOMPARE(displayName(QStringLiteral("fre")), QStringLiteral("French"));
        QCOMPARE(displayName(QStringLiteral("fra")), QStringLiteral("French"));
        QCOMPARE(displayName(QStringLiteral("ger")), QStringLiteral("German"));
        QCOMPARE(displayName(QStringLiteral("jpn")), QStringLiteral("Japanese"));
    }

    void testUnknownCodeReturnsRaw()
    {
        QCOMPARE(displayName(QStringLiteral("xyz")), QStringLiteral("xyz"));
    }

    void testEmptyCode()
    {
        QCOMPARE(displayName(QString {}), QString {});
    }

    void testInverseLookup()
    {
        QCOMPARE(codeForDisplayName(QStringLiteral("English")),
            QStringLiteral("eng"));
        QVERIFY(codeForDisplayName(QStringLiteral("Klingon")).isEmpty());
    }
};

QTEST_MAIN(TstLanguage)
#include "tst_language.moc"
