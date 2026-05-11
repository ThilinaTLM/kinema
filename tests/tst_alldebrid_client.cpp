// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/AllDebridClient.h"

#include "core/HttpError.h"
#include "TestDoubles.h"

#include <QTest>

using kinema::api::AllDebridClient;
using kinema::core::HttpError;
using kinema::tests::FakeHttpClient;
using kinema::tests::loadJsonFixture;

class TstAllDebridClient : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void user_addsBearerHeaderAndHitsV4()
    {
        FakeHttpClient http;
        http.jsonReplies = { loadJsonFixture("ad_user_premium.json") };

        AllDebridClient client(&http);
        client.setApiKey(QStringLiteral("ad-key"));

        const auto u = QCoro::waitFor(client.user());

        QCOMPARE(u.username, QStringLiteral("alice"));
        QCOMPARE(http.calls.size(), 1);
        const auto& call = http.calls.first();
        QVERIFY(call.usedRequest);
        QCOMPARE(call.request.url().path(), QStringLiteral("/v4/user"));
        QCOMPARE(call.request.rawHeader("Authorization"),
            QByteArrayLiteral("Bearer ad-key"));
    }

    void missingApiKey_throwsBeforeNetwork()
    {
        FakeHttpClient http;
        AllDebridClient client(&http);

        try {
            (void)QCoro::waitFor(client.user());
            QFAIL("expected HttpError");
        } catch (const HttpError& e) {
            QCOMPARE(e.httpStatus(), 401);
            QVERIFY(http.calls.isEmpty());
        }
    }

    void uploadMagnet_postsFormEncodedMagnetsArray()
    {
        FakeHttpClient http;
        http.jsonReplies = { loadJsonFixture("ad_add_magnet.json") };

        AllDebridClient client(&http);
        client.setApiKey(QStringLiteral("ad-key"));

        const auto r = QCoro::waitFor(
            client.uploadMagnet(QStringLiteral("magnet:?xt=urn:btih:AABB")));

        QCOMPARE(r.id, 123456LL);
        QCOMPARE(http.calls.size(), 1);
        const auto& call = http.calls.first();
        QCOMPARE(call.method, FakeHttpClient::Method::Post);
        QCOMPARE(call.request.url().path(),
            QStringLiteral("/v4/magnet/upload"));
        QCOMPARE(call.request
                     .header(QNetworkRequest::ContentTypeHeader)
                     .toString(),
            QStringLiteral("application/x-www-form-urlencoded"));
        QVERIFY(call.body.contains("magnets%5B%5D=")
            || call.body.contains("magnets[]="));
    }

    void magnetStatus_hitsV41()
    {
        FakeHttpClient http;
        http.jsonReplies = {
            loadJsonFixture("ad_magnet_status_ready.json"),
        };

        AllDebridClient client(&http);
        client.setApiKey(QStringLiteral("ad-key"));

        const auto s = QCoro::waitFor(client.magnetStatus(123456));

        QCOMPARE(s.statusCode, 4);
        QCOMPARE(http.calls.size(), 1);
        QCOMPARE(http.calls.first().request.url().path(),
            QStringLiteral("/v4.1/magnet/status"));
        QVERIFY(http.calls.first().body.contains("id=123456"));
    }

    void magnetFiles_postsIdArray()
    {
        FakeHttpClient http;
        http.jsonReplies = {
            loadJsonFixture("ad_magnet_files_flat.json"),
        };

        AllDebridClient client(&http);
        client.setApiKey(QStringLiteral("ad-key"));

        const auto files = QCoro::waitFor(client.magnetFiles(123456));

        QCOMPARE(files.size(), 1);
        QCOMPARE(http.calls.size(), 1);
        QCOMPARE(http.calls.first().request.url().path(),
            QStringLiteral("/v4/magnet/files"));
        const auto& body = http.calls.first().body;
        QVERIFY(body.contains("id%5B%5D=123456")
            || body.contains("id[]=123456"));
    }

    void unlockLink_immediate_returnsDownload()
    {
        FakeHttpClient http;
        http.jsonReplies = { loadJsonFixture("ad_unlock.json") };

        AllDebridClient client(&http);
        client.setApiKey(QStringLiteral("ad-key"));

        const auto u = QCoro::waitFor(client.unlockLink(
            QUrl(QStringLiteral("https://alldebrid.com/f/abcdefg"))));

        QVERIFY(!u.download.isEmpty());
        QCOMPARE(u.fileSize, 875773970LL);
        QCOMPARE(http.calls.size(), 1);
        QCOMPARE(http.calls.first().request.url().path(),
            QStringLiteral("/v4/link/unlock"));
    }

    void unlockLink_delayed_pollsAndReturnsFinal()
    {
        FakeHttpClient http;
        http.jsonReplies = {
            loadJsonFixture("ad_unlock_delayed.json"),
            loadJsonFixture("ad_delayed_ready.json"),
        };

        AllDebridClient client(&http);
        client.setApiKey(QStringLiteral("ad-key"));

        const auto u = QCoro::waitFor(client.unlockLink(
            QUrl(QStringLiteral("https://alldebrid.com/f/delayed-link"))));

        QCOMPARE(u.download,
            QUrl(QStringLiteral(
                "https://p1cjev.alldeb.ovh/dl/delayed/"
                "ubuntu-18.04.2-live-server-amd64.iso")));
        // Filename + size were carried forward from the first response.
        QCOMPARE(u.fileSize, 875773970LL);
        QCOMPARE(http.calls.size(), 2);
        QCOMPARE(http.calls.last().request.url().path(),
            QStringLiteral("/v4/link/delayed"));
    }

    void deleteMagnet_postsId()
    {
        FakeHttpClient http;
        http.byteReplies = { QByteArray() };

        AllDebridClient client(&http);
        client.setApiKey(QStringLiteral("ad-key"));

        QCoro::waitFor(client.deleteMagnet(123456));

        QCOMPARE(http.calls.size(), 1);
        QCOMPARE(http.calls.first().method, FakeHttpClient::Method::Post);
        QCOMPARE(http.calls.first().request.url().path(),
            QStringLiteral("/v4/magnet/delete"));
        QVERIFY(http.calls.first().body.contains("id=123456"));
    }
};

QTEST_MAIN(TstAllDebridClient)
#include "tst_alldebrid_client.moc"
