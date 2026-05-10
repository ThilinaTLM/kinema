// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "download/AssetSession.h"
#include "download/LocalMediaServer.h"

#include <QCoro/QCoroNetworkReply>
#include <QCoro/QCoroSignal>
#include <QCoro/QCoroTask>

#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QSignalSpy>
#include <QTest>

using namespace kinema::download;
namespace api = kinema::api;
using kinema::torrent::ByteRange;

namespace {

class FakeAssetSession : public AssetSession
{
public:
    explicit FakeAssetSession(QByteArray content,
        QString fileName = QStringLiteral("test.mkv"),
        QString assetId = QStringLiteral("asset-1"))
        : m_content(std::move(content))
        , m_fileName(std::move(fileName))
        , m_assetId(std::move(assetId))
    {
    }

    QString token() const override { return QStringLiteral("tok-1"); }
    QString assetId() const override { return m_assetId; }
    QString fileName() const override { return m_fileName; }
    qint64 fileSize() const override { return m_content.size(); }

    QCoro::Task<bool> ensureRange(ByteRange r) override
    {
        Q_UNUSED(r);
        ++ensureCount;
        co_return true;
    }
    QByteArray readRange(ByteRange r) const override
    {
        if (!r.isValid()) {
            return {};
        }
        const qint64 len = r.endInclusive - r.start + 1;
        return m_content.mid(static_cast<int>(r.start),
            static_cast<int>(len));
    }
    void touch() override { ++touchCount; }

    api::DownloadMode mode() const override { return m_mode; }
    void setMode(api::DownloadMode m) override { m_mode = m; }

    int ensureCount = 0;
    int touchCount = 0;

private:
    QByteArray m_content;
    QString m_fileName;
    QString m_assetId;
    api::DownloadMode m_mode = api::DownloadMode::OnDemand;
};

QByteArray fetchPath(const QUrl& url, const QByteArray& rangeHeader = {})
{
    QNetworkAccessManager nam;
    QNetworkRequest req(url);
    if (!rangeHeader.isEmpty()) {
        req.setRawHeader("Range", rangeHeader);
    }
    QNetworkReply* reply = nam.get(req);
    QCoro::waitFor(qCoro(reply).waitForFinished());
    const auto body = reply->readAll();
    reply->deleteLater();
    return body;
}

} // namespace

class TstLocalMediaServer : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void servesFullContentForFreshSession()
    {
        LocalMediaServer server;
        QVERIFY(server.listen());

        const QByteArray content("Hello, Kinema test payload");
        FakeAssetSession session(content);
        server.registerSession(&session);

        const QUrl url = server.urlFor(&session);
        const auto body = fetchPath(url);
        QCOMPARE(body, content);
        QVERIFY(session.ensureCount > 0);
        QVERIFY(session.touchCount > 0);
    }

    void respectsRangeRequests()
    {
        LocalMediaServer server;
        QVERIFY(server.listen());

        QByteArray content;
        for (int i = 0; i < 1024; ++i) {
            content.append(static_cast<char>(i & 0xff));
        }
        FakeAssetSession session(content);
        server.registerSession(&session);

        const QUrl url = server.urlFor(&session);
        QCOMPARE(url.path(), QStringLiteral("/stream/asset-1/test.mkv"));
        const auto body = fetchPath(url, "bytes=100-199");
        QCOMPARE(body.size(), 100);
        QCOMPARE(body, content.mid(100, 100));
    }

    void resolvesMissingLiveSessionThroughCallback()
    {
        LocalMediaServer server;
        QVERIFY(server.listen());

        const QByteArray content("Recovered payload");
        FakeAssetSession session(content,
            QStringLiteral("episode.mkv"),
            QStringLiteral("asset-recovered"));
        server.setSessionResolver(
            [&server, &session](const QString& assetId)
                -> QCoro::Task<AssetSession*> {
                if (assetId == session.assetId()) {
                    server.registerSession(&session);
                    co_return &session;
                }
                co_return nullptr;
            });

        QUrl url;
        url.setScheme(QStringLiteral("http"));
        url.setHost(QStringLiteral("127.0.0.1"));
        url.setPort(server.urlFor(&session).port());
        url.setPath(QStringLiteral("/stream/asset-recovered/episode.mkv"));

        const auto body = fetchPath(url);
        QCOMPARE(body, content);
        QVERIFY(session.ensureCount > 0);
    }

    void unknownAssetReturns404()
    {
        LocalMediaServer server;
        QVERIFY(server.listen());

        // Register/unregister a real session purely so we can read
        // the dynamically allocated port back out of the server.
        FakeAssetSession dummy(QByteArray("x"));
        server.registerSession(&dummy);
        const QUrl liveUrl = server.urlFor(&dummy);
        server.unregisterSession(&dummy);

        QUrl url;
        url.setScheme(QStringLiteral("http"));
        url.setHost(QStringLiteral("127.0.0.1"));
        url.setPort(liveUrl.port());
        url.setPath(QStringLiteral("/stream/missing-asset/file.mkv"));
        // The response body is whatever (likely empty); we just need
        // the request to complete without hanging.
        QNetworkAccessManager nam;
        QNetworkRequest req(url);
        QNetworkReply* reply = nam.get(req);
        QCoro::waitFor(qCoro(reply).waitForFinished());
        const int status = reply->attribute(
            QNetworkRequest::HttpStatusCodeAttribute).toInt();
        reply->deleteLater();
        QCOMPARE(status, 404);
    }
};

QTEST_MAIN(TstLocalMediaServer)
#include "tst_local_media_server.moc"
