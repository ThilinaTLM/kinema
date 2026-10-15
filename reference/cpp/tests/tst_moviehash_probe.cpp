// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/subtitles/MoviehashProbe.h"

#include "core/io/HttpClient.h"
#include "core/util/Moviehash.h"
#include "playback/events/PlaybackEvent.h"
#include "playback/events/PlaybackEventStream.h"
#include "TestDoubles.h"

#include <QByteArray>
#include <QCoreApplication>
#include <QSignalSpy>
#include <QTest>
#include <QUrl>
#include <QUuid>

#include <memory>
#include <variant>

using namespace kinema;
using namespace kinema::playback;
using namespace kinema::playback::events;
using namespace kinema::playback::subtitles;
using kinema::tests::FakeHttpClient;

namespace {

QByteArray deterministicBlock(char seed)
{
    QByteArray b(65536, seed);
    return b;
}

QList<QPair<QByteArray, QByteArray>> contentLengthHeaders(qint64 size)
{
    return { { QByteArrayLiteral("Content-Length"),
        QByteArray::number(size) } };
}

PlaybackSessionId fresh()
{
    return QUuid::createUuid();
}

void drain(int rounds = 5)
{
    for (int i = 0; i < rounds; ++i) {
        QCoreApplication::processEvents();
        QCoreApplication::sendPostedEvents();
    }
}

template<class T>
bool spyHas(const QSignalSpy& spy)
{
    for (const auto& row : spy) {
        const auto e = row.first().value<PlaybackEvent>();
        if (std::holds_alternative<T>(e)) {
            return true;
        }
    }
    return false;
}

template<class T>
T extractEvent(const QSignalSpy& spy)
{
    for (const auto& row : spy) {
        const auto e = row.first().value<PlaybackEvent>();
        if (std::holds_alternative<T>(e)) {
            return std::get<T>(e);
        }
    }
    return T {};
}

} // namespace

class TstMoviehashProbe : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void init()
    {
        m_http = std::make_unique<FakeHttpClient>();
        m_events = std::make_unique<PlaybackEventStream>();
        m_probe = std::make_unique<MoviehashProbe>(*m_events,
            m_http.get());
    }

    void cleanup()
    {
        m_probe.reset();
        m_events.reset();
        m_http.reset();
    }

    // -----------------------------------------------------------
    // Non-HTTPS URLs are skipped silently (no HEAD, no event).
    // -----------------------------------------------------------
    void nonHttpsIsSkipped()
    {
        QSignalSpy spy(m_events.get(),
            &PlaybackEventStream::eventPublished);

        m_events->publish(PlayableUrlReady {
            fresh(), QString {},
            QUrl(QStringLiteral("http://127.0.0.1/stream"))
        });
        drain();

        QCOMPARE(m_http->calls.size(), 0);
        // The PlayableUrlReady we published is the only event;
        // no MoviehashComputed.
        QVERIFY(!spyHas<MoviehashComputed>(spy));
    }

    // -----------------------------------------------------------
    // Standard happy path: HEAD -> head Range GET -> tail Range
    // GET -> MoviehashComputed published with the expected hex.
    // -----------------------------------------------------------
    void happyPathPublishesComputedHash()
    {
        const QByteArray head = deterministicBlock('A');
        const QByteArray tail = deterministicBlock('B');
        constexpr qint64 size = 200000;
        const QString expectedHex
            = core::moviehash::compute(head, tail, size);
        QVERIFY(!expectedHex.isEmpty());

        m_http->headerReplies.append(contentLengthHeaders(size));
        m_http->byteReplies.append(head);
        m_http->byteReplies.append(tail);

        QSignalSpy spy(m_events.get(),
            &PlaybackEventStream::eventPublished);

        const auto sid = fresh();
        m_events->publish(PlayableUrlReady {
            sid, QString {},
            QUrl(QStringLiteral("https://hoster/stream.mkv")) });
        drain(8);

        QVERIFY(spyHas<MoviehashComputed>(spy));
        const auto m = extractEvent<MoviehashComputed>(spy);
        QCOMPARE(m.sessionId, sid);
        QCOMPARE(m.hex, expectedHex);
    }

    // -----------------------------------------------------------
    // Too-small Content-Length aborts before the Range GETs.
    // -----------------------------------------------------------
    void smallContentLengthIsRejected()
    {
        m_http->headerReplies.append(contentLengthHeaders(1000));

        QSignalSpy spy(m_events.get(),
            &PlaybackEventStream::eventPublished);

        m_events->publish(PlayableUrlReady {
            fresh(), QString {},
            QUrl(QStringLiteral("https://hoster/tiny.mkv")) });
        drain();

        QVERIFY(!spyHas<MoviehashComputed>(spy));
        // Only HEAD was issued; no Range GETs.
        QCOMPARE(m_http->calls.size(), 1);
        QCOMPARE(m_http->calls.first().method,
            FakeHttpClient::Method::Head);
    }

    // -----------------------------------------------------------
    // Missing Content-Length header behaves like size==0 -> abort.
    // -----------------------------------------------------------
    void missingContentLengthAborts()
    {
        m_http->headerReplies.append(
            QList<QPair<QByteArray, QByteArray>> {});
        QSignalSpy spy(m_events.get(),
            &PlaybackEventStream::eventPublished);
        m_events->publish(PlayableUrlReady {
            fresh(), QString {},
            QUrl(QStringLiteral("https://hoster/x.mkv")) });
        drain();
        QVERIFY(!spyHas<MoviehashComputed>(spy));
    }

    // -----------------------------------------------------------
    // A second PlayableUrlReady supersedes the in-flight probe;
    // the late response from the prior URL is dropped.
    // -----------------------------------------------------------
    void supersedingProbeDropsStaleResults()
    {
        // No headers/replies queued: the first probe will get
        // empty replies and won't publish. Then the second probe
        // gets a real response; only its hex should appear.
        m_http->headerReplies.append(
            QList<QPair<QByteArray, QByteArray>> {});
        m_http->headerReplies.append(contentLengthHeaders(200000));

        const QByteArray head = deterministicBlock('C');
        const QByteArray tail = deterministicBlock('D');
        m_http->byteReplies.append(head);
        m_http->byteReplies.append(tail);

        QSignalSpy spy(m_events.get(),
            &PlaybackEventStream::eventPublished);

        const auto sid1 = fresh();
        const auto sid2 = fresh();
        m_events->publish(PlayableUrlReady {
            sid1, QString {},
            QUrl(QStringLiteral("https://hoster/a.mkv")) });
        m_events->publish(PlayableUrlReady {
            sid2, QString {},
            QUrl(QStringLiteral("https://hoster/b.mkv")) });
        drain(8);

        QVERIFY(spyHas<MoviehashComputed>(spy));
        const auto m = extractEvent<MoviehashComputed>(spy);
        QCOMPARE(m.sessionId, sid2);
    }

    // -----------------------------------------------------------
    // No HttpClient => probe is a silent no-op.
    // -----------------------------------------------------------
    void noHttpClientIsNoop()
    {
        auto eventsLocal = std::make_unique<PlaybackEventStream>();
        auto probeLocal = std::make_unique<MoviehashProbe>(
            *eventsLocal, /*http=*/nullptr);

        QSignalSpy spy(eventsLocal.get(),
            &PlaybackEventStream::eventPublished);
        eventsLocal->publish(PlayableUrlReady {
            fresh(), QString {},
            QUrl(QStringLiteral("https://hoster/x.mkv")) });
        drain();
        QVERIFY(!spyHas<MoviehashComputed>(spy));
    }

private:
    std::unique_ptr<FakeHttpClient> m_http;
    std::unique_ptr<PlaybackEventStream> m_events;
    std::unique_ptr<MoviehashProbe> m_probe;
};

QTEST_MAIN(TstMoviehashProbe)
#include "tst_moviehash_probe.moc"
