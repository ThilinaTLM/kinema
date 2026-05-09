// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/RealDebridClient.h"

#include "core/HttpError.h"
#include "TestDoubles.h"

#include <QTest>

using kinema::api::RealDebridClient;
using kinema::core::HttpError;
using kinema::tests::FakeHttpClient;
using kinema::tests::loadJsonFixture;

class TstRealDebridClient : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void testUserEndpointAddsAuthorizationHeader()
    {
        FakeHttpClient http;
        http.jsonReplies = { loadJsonFixture("rd_user_premium.json") };

        RealDebridClient client(&http);
        client.setToken(QStringLiteral("rd-token"));

        const auto user = QCoro::waitFor(client.user());

        QCOMPARE(user.username, QStringLiteral("alice"));
        QCOMPARE(http.calls.size(), 1);
        QVERIFY(http.calls.first().usedRequest);
        QCOMPARE(http.calls.first().request.url().path(),
            QStringLiteral("/rest/1.0/user"));
        QCOMPARE(http.calls.first().request.rawHeader("Authorization"),
            QByteArrayLiteral("Bearer rd-token"));
    }

    void testMissingTokenFailsBeforeRequest()
    {
        FakeHttpClient http;
        RealDebridClient client(&http);

        try {
            (void)QCoro::waitFor(client.user());
            QFAIL("expected HttpError");
        } catch (const HttpError& e) {
            QCOMPARE(e.httpStatus(), 401);
            QVERIFY(http.calls.isEmpty());
        }
    }

    void instantAvailability_buildsHashScopedUrl()
    {
        FakeHttpClient http;
        http.jsonReplies = { loadJsonFixture("rd_instant_availability_cached.json") };

        RealDebridClient client(&http);
        client.setToken(QStringLiteral("rd-token"));

        const auto av = QCoro::waitFor(client.instantAvailability(
            QStringLiteral("AABB1122CCDD3344EEFF5566778899AABBCCDDEE")));

        QVERIFY(av.cached());
        QCOMPARE(http.calls.size(), 1);
        QCOMPARE(http.calls.first().request.url().path(),
            QStringLiteral("/rest/1.0/torrents/instantAvailability/"
                           "aabb1122ccdd3344eeff5566778899aabbccddee"));
        QCOMPARE(http.calls.first().request.rawHeader("Authorization"),
            QByteArrayLiteral("Bearer rd-token"));
    }

    void addMagnet_postsFormEncodedMagnet()
    {
        FakeHttpClient http;
        http.jsonReplies = { loadJsonFixture("rd_add_magnet.json") };

        RealDebridClient client(&http);
        client.setToken(QStringLiteral("rd-token"));

        const auto magnet = QStringLiteral(
            "magnet:?xt=urn:btih:AABB&dn=movie");
        const auto r = QCoro::waitFor(client.addMagnet(magnet));

        QCOMPARE(r.id, QStringLiteral("ABC123ID"));
        QCOMPARE(http.calls.size(), 1);
        const auto& call = http.calls.first();
        QCOMPARE(call.method, FakeHttpClient::Method::Post);
        QCOMPARE(call.request.url().path(),
            QStringLiteral("/rest/1.0/torrents/addMagnet"));
        QCOMPARE(call.request.header(QNetworkRequest::ContentTypeHeader)
                     .toString(),
            QStringLiteral("application/x-www-form-urlencoded"));
        QVERIFY(call.body.contains("magnet="));
    }

    void selectFiles_emptyListSendsAll()
    {
        FakeHttpClient http;
        http.byteReplies = { QByteArray() };

        RealDebridClient client(&http);
        client.setToken(QStringLiteral("rd-token"));

        QCoro::waitFor(client.selectFiles(QStringLiteral("ABC123ID"), {}));

        QCOMPARE(http.calls.size(), 1);
        const auto& call = http.calls.first();
        QCOMPARE(call.method, FakeHttpClient::Method::Post);
        QCOMPARE(call.request.url().path(),
            QStringLiteral("/rest/1.0/torrents/selectFiles/ABC123ID"));
        QVERIFY(call.body.contains("files=all"));
    }

    void selectFiles_listJoinsCommaSeparated()
    {
        FakeHttpClient http;
        http.byteReplies = { QByteArray() };

        RealDebridClient client(&http);
        client.setToken(QStringLiteral("rd-token"));

        QCoro::waitFor(client.selectFiles(
            QStringLiteral("ABC123ID"), QList<int> { 1, 3, 5 }));

        QCOMPARE(http.calls.size(), 1);
        QVERIFY(http.calls.first().body.contains("files=1%2C3%2C5")
            || http.calls.first().body.contains("files=1,3,5"));
    }

    void unrestrictLink_returnsDownloadUrl()
    {
        FakeHttpClient http;
        http.jsonReplies = { loadJsonFixture("rd_unrestrict_link.json") };

        RealDebridClient client(&http);
        client.setToken(QStringLiteral("rd-token"));

        const auto u = QCoro::waitFor(client.unrestrictLink(
            QUrl(QStringLiteral("https://real-debrid.com/d/X/Y.mkv"))));

        QVERIFY(!u.download.isEmpty());
        QCOMPARE(u.fileSize, qint64(2147483648));
        QCOMPARE(http.calls.size(), 1);
        QCOMPARE(http.calls.first().request.url().path(),
            QStringLiteral("/rest/1.0/unrestrict/link"));
    }

    void deleteTorrent_issuesDelete()
    {
        FakeHttpClient http;
        http.byteReplies = { QByteArray() };

        RealDebridClient client(&http);
        client.setToken(QStringLiteral("rd-token"));

        QCoro::waitFor(client.deleteTorrent(QStringLiteral("ABC123ID")));

        QCOMPARE(http.calls.size(), 1);
        const auto& call = http.calls.first();
        QCOMPARE(call.method, FakeHttpClient::Method::Delete);
        QCOMPARE(call.request.url().path(),
            QStringLiteral("/rest/1.0/torrents/delete/ABC123ID"));
    }
};

QTEST_MAIN(TstRealDebridClient)
#include "tst_realdebrid_client.moc"
