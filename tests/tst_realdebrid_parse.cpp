// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

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

    void instantAvailability_cached_listsVariants()
    {
        const auto doc = loadFixture("rd_instant_availability_cached.json");
        const auto av = realdebrid::parseInstantAvailability(doc,
            QStringLiteral("AABB1122CCDD3344EEFF5566778899AABBCCDDEE"));

        QVERIFY(av.cached());
        QCOMPARE(av.variants.size(), 2);
        QCOMPARE(av.variants.at(0).files.size(), 2);
        const auto& f0 = av.variants.at(0).files.at(0);
        QCOMPARE(f0.fileId, 1);
        QCOMPARE(f0.filename, QStringLiteral("Sample.Movie.2023.1080p.mkv"));
        QCOMPARE(f0.sizeBytes, qint64(2147483648));
    }

    void instantAvailability_uncached_returnsEmpty()
    {
        const auto doc = loadFixture("rd_instant_availability_uncached.json");
        const auto av = realdebrid::parseInstantAvailability(doc,
            QStringLiteral("ff00112233445566778899aabbccddeeff001122"));
        QVERIFY(!av.cached());
        QVERIFY(av.variants.isEmpty());
    }

    void addMagnet_extractsId()
    {
        const auto doc = loadFixture("rd_add_magnet.json");
        const auto r = realdebrid::parseAddMagnet(doc);
        QCOMPARE(r.id, QStringLiteral("ABC123ID"));
        QVERIFY(!r.uri.isEmpty());
    }

    void addMagnet_throwsOnMissingId()
    {
        const auto doc = QJsonDocument::fromJson(QByteArray("{}"));
        QVERIFY_EXCEPTION_THROWN(
            realdebrid::parseAddMagnet(doc),
            kinema::core::HttpError);
    }

    void torrentInfo_listsFilesAndLinks()
    {
        const auto doc = loadFixture("rd_torrent_info_downloaded.json");
        const auto t = realdebrid::parseTorrentInfo(doc);
        QCOMPARE(t.id, QStringLiteral("ABC123ID"));
        QCOMPARE(t.status, QStringLiteral("downloaded"));
        QCOMPARE(t.progress, 100);
        QCOMPARE(t.files.size(), 2);
        QVERIFY(t.files.at(0).selected);
        QVERIFY(!t.files.at(1).selected);
        QCOMPARE(t.links.size(), 1);
    }

    void unrestrictLink_extractsDownload()
    {
        const auto doc = loadFixture("rd_unrestrict_link.json");
        const auto u = realdebrid::parseUnrestrictedLink(doc);
        QCOMPARE(u.filename, QStringLiteral("Sample.Movie.2023.1080p.mkv"));
        QCOMPARE(u.fileSize, qint64(2147483648));
        QVERIFY(u.streamable);
        QVERIFY(!u.download.isEmpty());
    }

    void unrestrictLink_throwsOnMissingDownload()
    {
        const auto doc = QJsonDocument::fromJson(QByteArray("{}"));
        QVERIFY_EXCEPTION_THROWN(
            realdebrid::parseUnrestrictedLink(doc),
            kinema::core::HttpError);
    }
};

QTEST_APPLESS_MAIN(TstRealDebridParse)
#include "tst_realdebrid_parse.moc"
