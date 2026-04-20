// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "api/RealDebridParse.h"
#include "core/HttpError.h"

#include <QFile>
#include <QJsonDocument>
#include <QTest>

using namespace kinema::api;

class TstRealDebridParse : public QObject
{
    Q_OBJECT
private:
    static QJsonDocument loadFixture(const char* name)
    {
        QFile f(QStringLiteral(KINEMA_TEST_FIXTURES_DIR) + QLatin1Char('/')
            + QString::fromLatin1(name));
        if (!f.open(QIODevice::ReadOnly)) {
            qFatal("Cannot open fixture %s", name);
        }
        return QJsonDocument::fromJson(f.readAll());
    }

private Q_SLOTS:
    void premium_parsesUsernameEmailTypeAndExpiration()
    {
        const auto doc = loadFixture("rd_user_premium.json");
        const auto u = realdebrid::parseUser(doc);

        QCOMPARE(u.username, QStringLiteral("alice"));
        QCOMPARE(u.email, QStringLiteral("alice@example.com"));
        QCOMPARE(u.type, QStringLiteral("premium"));
        QVERIFY(u.premiumUntil.has_value());
        QCOMPARE(u.premiumUntil->date(), QDate(2026, 11, 14));
    }

    void free_hasNoExpiration()
    {
        const auto doc = loadFixture("rd_user_free.json");
        const auto u = realdebrid::parseUser(doc);

        QCOMPARE(u.username, QStringLiteral("bob"));
        QCOMPARE(u.type, QStringLiteral("free"));
        QVERIFY(!u.premiumUntil.has_value());
    }

    void throwsOnNonObjectDocument()
    {
        const auto doc = QJsonDocument::fromJson(QByteArray("[1,2,3]"));
        QVERIFY_EXCEPTION_THROWN(
            realdebrid::parseUser(doc),
            kinema::core::HttpError);
    }

    void toleratesMissingFields()
    {
        const auto doc = QJsonDocument::fromJson(QByteArray("{}"));
        const auto u = realdebrid::parseUser(doc);
        QVERIFY(u.username.isEmpty());
        QVERIFY(u.email.isEmpty());
        QVERIFY(u.type.isEmpty());
        QVERIFY(!u.premiumUntil.has_value());
    }
};

QTEST_APPLESS_MAIN(TstRealDebridParse)
#include "tst_realdebrid_parse.moc"
