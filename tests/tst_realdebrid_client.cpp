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
};

QTEST_MAIN(TstRealDebridClient)
#include "tst_realdebrid_client.moc"
