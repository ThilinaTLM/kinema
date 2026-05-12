// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/AllDebridParse.h"

#include "core/io/HttpError.h"
#include "TestDoubles.h"

#include <QTest>

using namespace kinema::api::alldebrid;
using kinema::core::HttpError;
using kinema::tests::loadJsonFixture;

class TstAllDebridParse : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void user_premium_parses()
    {
        const auto u = parseUser(loadJsonFixture("ad_user_premium.json"));
        QCOMPARE(u.username, QStringLiteral("alice"));
        QCOMPARE(u.email, QStringLiteral("alice@example.com"));
        QVERIFY(u.isPremium);
        QVERIFY(!u.isTrial);
        QVERIFY(u.premiumUntil.has_value());
        QCOMPARE(u.premiumUntil->toSecsSinceEpoch(), 1893456000LL);
    }

    void user_free_parses()
    {
        const auto u = parseUser(loadJsonFixture("ad_user_free.json"));
        QCOMPARE(u.username, QStringLiteral("bob"));
        QVERIFY(!u.isPremium);
        QVERIFY(!u.premiumUntil.has_value());
    }

    void error_envelope_throws_with_code()
    {
        try {
            (void)parseUser(loadJsonFixture("ad_error_bad_apikey.json"));
            QFAIL("expected HttpError");
        } catch (const HttpError& e) {
            // AUTH_* maps to 401.
            QCOMPARE(e.httpStatus(), 401);
            QVERIFY(e.message().contains(QStringLiteral("AUTH_BAD_APIKEY")));
            QVERIFY(e.message().contains(QStringLiteral("invalid")));
        }
    }

    void addMagnet_parses()
    {
        const auto r = parseAddMagnet(loadJsonFixture("ad_add_magnet.json"));
        QCOMPARE(r.id, 123456LL);
        QCOMPARE(r.hash,
            QStringLiteral("842783e3005495d5d1637f5364b59343c7844707"));
        QVERIFY(r.ready);
        QCOMPARE(r.sizeBytes, 875773970LL);
    }

    void addMagnet_rowError_throws()
    {
        try {
            (void)parseAddMagnet(loadJsonFixture("ad_add_magnet_error.json"));
            QFAIL("expected HttpError");
        } catch (const HttpError& e) {
            QVERIFY(e.message().contains(QStringLiteral("MAGNET_INVALID_URI")));
        }
    }

    void magnetStatus_ready_parses()
    {
        const auto s = parseMagnetStatus(
            loadJsonFixture("ad_magnet_status_ready.json"));
        QCOMPARE(s.statusCode, 4);
        QCOMPARE(s.id, 123456LL);
        QCOMPARE(s.filename,
            QStringLiteral("ubuntu-18.04.2-live-server-amd64.iso"));
    }

    void magnetStatus_singleObjectShape_parses()
    {
        // AllDebrid /v4.1/magnet/status returns the row as an object
        // (not a single-element array) when called with a specific
        // `id`. Make sure the parser accepts that shape.
        const auto s = parseMagnetStatus(
            loadJsonFixture("ad_magnet_status_ready_single_object.json"));
        QCOMPARE(s.id, 123456LL);
        QCOMPARE(s.statusCode, 4);
        QCOMPARE(s.filename,
            QStringLiteral("ubuntu-18.04.2-live-server-amd64.iso"));
    }

    void magnetStatus_downloading_parses()
    {
        const auto s = parseMagnetStatus(
            loadJsonFixture("ad_magnet_status_downloading.json"));
        QCOMPARE(s.statusCode, 1);
        QCOMPARE(s.downloaded, 255400192LL);
        QCOMPARE(s.downloadSpeed, 18874368LL);
        QCOMPARE(s.seeders, 7);
    }

    void magnetFiles_flat_returnsOneLeaf()
    {
        const auto files = parseMagnetFiles(
            loadJsonFixture("ad_magnet_files_flat.json"));
        QCOMPARE(files.size(), 1);
        QCOMPARE(files.first().path,
            QStringLiteral("ubuntu-18.04.2-live-server-amd64.iso"));
        QCOMPARE(files.first().bytes, 875773970LL);
        QCOMPARE(files.first().hosterLink,
            QUrl(QStringLiteral("https://alldebrid.com/f/abcdefg")));
    }

    void magnetFiles_nested_flattenedWithJoinedPaths()
    {
        const auto files = parseMagnetFiles(
            loadJsonFixture("ad_magnet_files_nested.json"));
        QCOMPARE(files.size(), 4);

        const QStringList paths = {
            files[0].path, files[1].path, files[2].path, files[3].path,
        };
        QVERIFY(paths.contains(
            QStringLiteral("Show.S01.1080p/Show.S01E01.1080p.mkv")));
        QVERIFY(paths.contains(
            QStringLiteral("Show.S01.1080p/Show.S01E02.1080p.mkv")));
        QVERIFY(paths.contains(
            QStringLiteral("Show.S01.1080p/Subs/Show.S01E01.en.srt")));
        QVERIFY(paths.contains(QStringLiteral("readme.txt")));
    }

    void unlock_immediate_parses()
    {
        const auto u = parseUnlock(loadJsonFixture("ad_unlock.json"));
        QVERIFY(!u.download.isEmpty());
        QCOMPARE(u.fileSize, 875773970LL);
        QCOMPARE(u.delayedId, 0LL);
    }

    void unlock_delayed_returnsDelayedId()
    {
        const auto u = parseUnlock(loadJsonFixture("ad_unlock_delayed.json"));
        QVERIFY(u.download.isEmpty());
        QCOMPARE(u.delayedId, 2277564LL);
        QCOMPARE(u.fileSize, 875773970LL);
    }

    void delayed_processing_returnsEmptyDownload()
    {
        const auto u = parseDelayed(
            loadJsonFixture("ad_delayed_processing.json"));
        QVERIFY(u.download.isEmpty());
    }

    void delayed_ready_returnsUrl()
    {
        const auto u = parseDelayed(loadJsonFixture("ad_delayed_ready.json"));
        QCOMPARE(u.download,
            QUrl(QStringLiteral(
                "https://p1cjev.alldeb.ovh/dl/delayed/"
                "ubuntu-18.04.2-live-server-amd64.iso")));
    }
};

QTEST_GUILESS_MAIN(TstAllDebridParse)
#include "tst_alldebrid_parse.moc"
