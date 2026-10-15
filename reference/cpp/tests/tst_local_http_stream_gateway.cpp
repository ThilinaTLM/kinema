// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/ports/ByteRangeSource.h"
#include "playback/streaming/LocalHttpStreamGateway.h"

#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QSignalSpy>
#include <QTest>

#include <QCoro/QCoroNetworkReply>
#include <QCoro/QCoroSignal>
#include <QCoro/QCoroTask>

#include <vector>

using kinema::core::ByteRange;
using kinema::playback::ports::ByteRangeSource;
using kinema::playback::streaming::LocalHttpStreamGateway;

namespace {

class FakeByteRangeSource : public ByteRangeSource
{
public:
    explicit FakeByteRangeSource(QByteArray content,
                                 QString fileName = QStringLiteral("test.mkv"),
                                 QString assetId = QStringLiteral("asset-1"))
        : m_content(std::move(content))
        , m_fileName(std::move(fileName))
        , m_assetId(std::move(assetId))
    { }

    QString assetId() const override { return m_assetId; }
    QString fileName() const override { return m_fileName; }
    qint64 fileSize() const override { return m_content.size(); }

    QCoro::Task<bool> ensureRange(ByteRange r) override
    {
        ++ensureCount;
        ensureRanges.push_back(r);
        co_return true;
    }
    QByteArray readRange(ByteRange r) const override
    {
        if (!r.isValid()) {
            return {};
        }
        const qint64 len = r.endInclusive - r.start + 1;
        return m_content.mid(static_cast<int>(r.start), static_cast<int>(len));
    }
    void touch() override { ++touchCount; }

    int ensureCount = 0;
    int touchCount = 0;
    std::vector<ByteRange> ensureRanges;

private:
    QByteArray m_content;
    QString m_fileName;
    QString m_assetId;
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

int fetchStatus(const QUrl& url,
                const QByteArray& method = "GET",
                const QByteArray& rangeHeader = {})
{
    QNetworkAccessManager nam;
    QNetworkRequest req(url);
    if (!rangeHeader.isEmpty()) {
        req.setRawHeader("Range", rangeHeader);
    }
    QNetworkReply* reply = nullptr;
    if (method == "HEAD") {
        reply = nam.head(req);
    } else {
        reply = nam.get(req);
    }
    QCoro::waitFor(qCoro(reply).waitForFinished());
    const int status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    reply->deleteLater();
    return status;
}

} // namespace

class TstLocalHttpStreamGateway : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void servesFullContentForFreshSource()
    {
        LocalHttpStreamGateway gw;
        QVERIFY(gw.listen());

        const QByteArray content("Hello, Kinema test payload");
        FakeByteRangeSource source(content);
        const QUrl url = gw.expose(source);

        const auto body = fetchPath(url);
        QCOMPARE(body, content);
        QVERIFY(source.ensureCount > 0);
        QVERIFY(source.touchCount > 0);
    }

    void urlForDoesNotDoublePercentEncode()
    {
        LocalHttpStreamGateway gw;
        QVERIFY(gw.listen());

        FakeByteRangeSource source(QByteArray("x"),
                                   QStringLiteral("The Show (1080p) [foo].mkv"),
                                   QStringLiteral("asset-1"));
        const QUrl url = gw.expose(source);
        QCOMPARE(url.path(), QStringLiteral("/stream/asset-1/The Show (1080p) [foo].mkv"));
        const QByteArray encoded = url.toEncoded();
        QVERIFY(!encoded.contains("%2520"));
        QVERIFY(encoded.contains("%20"));
    }

    void respectsRangeRequests()
    {
        LocalHttpStreamGateway gw;
        QVERIFY(gw.listen());

        QByteArray content;
        for (int i = 0; i < 1024; ++i) {
            content.append(static_cast<char>(i & 0xff));
        }
        FakeByteRangeSource source(content);
        const QUrl url = gw.expose(source);
        QCOMPARE(url.path(), QStringLiteral("/stream/asset-1/test.mkv"));
        const auto body = fetchPath(url, "bytes=100-199");
        QCOMPARE(body.size(), 100);
        QCOMPARE(body, content.mid(100, 100));
    }

    void invalidRangeReturns416()
    {
        LocalHttpStreamGateway gw;
        QVERIFY(gw.listen());
        FakeByteRangeSource source(QByteArray(1024, 'x'));
        const QUrl url = gw.expose(source);
        QCOMPARE(fetchStatus(url, "GET", "bytes=99999999-"), 416);
    }

    void headRequestHasContent()
    {
        LocalHttpStreamGateway gw;
        QVERIFY(gw.listen());
        FakeByteRangeSource source(QByteArray(1024, 'x'));
        const QUrl url = gw.expose(source);
        QCOMPARE(fetchStatus(url, "HEAD"), 200);
    }

    void resolvesMissingLiveSessionThroughCallback()
    {
        LocalHttpStreamGateway gw;
        QVERIFY(gw.listen());

        const QByteArray content("Recovered payload");
        FakeByteRangeSource source(
            content, QStringLiteral("episode.mkv"), QStringLiteral("asset-recovered"));
        // Expose it once to learn the port, then revoke and rely on
        // the resolver to "rehydrate" it.
        QUrl url = gw.expose(source);
        gw.revoke(source);

        gw.setSessionResolver(
            [&gw, &source](const QString& assetId) -> QCoro::Task<ByteRangeSource*> {
                if (assetId == source.assetId()) {
                    gw.expose(source);
                    co_return &source;
                }
                co_return nullptr;
            });

        const auto body = fetchPath(url);
        QCOMPARE(body, content);
        QVERIFY(source.ensureCount > 0);
    }

    void nonRangeGetServesEntireBodyViaChunkedReads()
    {
        LocalHttpStreamGateway gw;
        QVERIFY(gw.listen());

        const int kSize = 6 * 1024 * 1024;
        QByteArray content;
        content.resize(kSize);
        for (int i = 0; i < kSize; ++i) {
            content[i] = static_cast<char>((i * 31) & 0xff);
        }
        FakeByteRangeSource source(content, QStringLiteral("big.mkv"), QStringLiteral("asset-big"));
        const QUrl url = gw.expose(source);
        const auto body = fetchPath(url);
        QCOMPARE(body.size(), content.size());
        QCOMPARE(body, content);

        QVERIFY(source.ensureRanges.size() >= 2);
        for (const auto& r : source.ensureRanges) {
            const qint64 len = r.endInclusive - r.start + 1;
            QVERIFY(len < kSize);
        }
    }

    void unknownAssetReturns404()
    {
        LocalHttpStreamGateway gw;
        QVERIFY(gw.listen());

        FakeByteRangeSource dummy(QByteArray("x"));
        const QUrl liveUrl = gw.expose(dummy);
        gw.revoke(dummy);

        QUrl url;
        url.setScheme(QStringLiteral("http"));
        url.setHost(QStringLiteral("127.0.0.1"));
        url.setPort(liveUrl.port());
        url.setPath(QStringLiteral("/stream/missing-asset/file.mkv"));
        QCOMPARE(fetchStatus(url), 404);
    }
};

QTEST_MAIN(TstLocalHttpStreamGateway)
#include "tst_local_http_stream_gateway.moc"
