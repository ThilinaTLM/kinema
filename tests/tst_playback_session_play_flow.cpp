// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "playback/events/PlaybackEvent.h"
#include "playback/events/PlaybackEventStream.h"
#include "playback/session/PlaybackSession.h"
#include "playback/session/PlaybackStateMachine.h"

#include <QSignalSpy>
#include <QTest>
#include <QUuid>

#include <variant>

using namespace kinema;
using namespace kinema::playback;
using namespace kinema::playback::events;
using namespace kinema::playback::session;

namespace {

domain::Stream makeStream(const QString& release)
{
    domain::Stream s;
    s.releaseName = release;
    s.qualityLabel = QStringLiteral("1080p");
    s.resolution = QStringLiteral("1080p");
    s.infoHash = QStringLiteral("abc");
    return s;
}

domain::PlaybackContext makeCtx(const QString& imdb)
{
    domain::PlaybackContext ctx;
    domain::PlaybackKey key;
    key.kind = domain::MediaKind::Movie;
    key.imdbId = imdb;
    ctx.key = key;
    ctx.title = imdb;
    return ctx;
}

template<class T>
bool spyHas(const QSignalSpy& spy)
{
    for (const auto& row : spy) {
        if (std::holds_alternative<T>(row.first().value<PlaybackEvent>())) {
            return true;
        }
    }
    return false;
}

template<class T>
T extract(const QSignalSpy& spy)
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

class TstPlaybackSessionPlayFlow : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void startPublishesPlaybackRequested()
    {
        PlaybackEventStream stream;
        PlaybackSession session(stream);
        QVERIFY(!session.id().isNull());
        QCOMPARE(session.state(), PlaybackState::Idle);

        QSignalSpy spy(&stream, &PlaybackEventStream::eventPublished);
        session.start(makeStream(QStringLiteral("S")),
            makeCtx(QStringLiteral("tt1")));

        QVERIFY(spyHas<PlaybackRequested>(spy));
        const auto req = extract<PlaybackRequested>(spy);
        QCOMPARE(req.sessionId, session.id());
        QCOMPARE(req.ctx.key.imdbId, QStringLiteral("tt1"));
        QCOMPARE(session.state(), PlaybackState::ResolvingSource);
    }

    void backendOverrideIsCarried()
    {
        PlaybackEventStream stream;
        PlaybackSession session(stream);
        session.start(makeStream(QStringLiteral("S")),
            makeCtx(QStringLiteral("tt1")),
            domain::DownloadBackendKind::RealDebridHttp);
        QVERIFY(session.backendOverride().has_value());
        QCOMPARE(*session.backendOverride(),
            domain::DownloadBackendKind::RealDebridHttp);
    }

    void resolveAndLoadProgressStateMachine()
    {
        PlaybackEventStream stream;
        PlaybackSession session(stream);
        session.start(makeStream(QStringLiteral("S")),
            makeCtx(QStringLiteral("tt1")));

        QSignalSpy spy(&stream, &PlaybackEventStream::eventPublished);
        domain::AssetRef asset;
        asset.key = makeCtx(QStringLiteral("tt1")).key;
        asset.infoHash = QStringLiteral("abc");
        session.markSourceResolved(asset);
        QCOMPARE(session.state(), PlaybackState::PreparingTransfer);
        QVERIFY(spyHas<SourceResolved>(spy));

        session.markPlayableUrl(QStringLiteral("aid"),
            QUrl(QStringLiteral("http://127.0.0.1/x")));
        QCOMPARE(session.state(), PlaybackState::LoadingPlayer);
        QVERIFY(spyHas<PlayableUrlReady>(spy));

        session.markPlayerLoading();
        QVERIFY(spyHas<PlayerLoading>(spy));

        session.markPlayerLoaded();
        QCOMPARE(session.state(), PlaybackState::Playing);
        QVERIFY(spyHas<PlayerLoaded>(spy));

        session.markPositionTick(42.0);
        QVERIFY(spyHas<PositionTicked>(spy));

        session.markDuration(7200.0);
        QVERIFY(spyHas<DurationChanged>(spy));
    }

    void stopByUserPublishesUserStopAndTerminates()
    {
        PlaybackEventStream stream;
        PlaybackSession session(stream);
        session.start(makeStream(QStringLiteral("S")),
            makeCtx(QStringLiteral("tt1")));

        QSignalSpy spy(&stream, &PlaybackEventStream::eventPublished);
        QVERIFY(session.stopByUser());

        QVERIFY(spyHas<PlaybackEnded>(spy));
        const auto ended = extract<PlaybackEnded>(spy);
        QCOMPARE(ended.reason, PlaybackEndReason::UserStop);
        QCOMPARE(ended.sessionId, session.id());

        // Stop is idempotent on terminal sessions.
        QSignalSpy spy2(&stream, &PlaybackEventStream::eventPublished);
        QVERIFY(!session.stopByUser());
        QCOMPARE(spy2.count(), 0);
    }

    void markEndedNaturalEofMovesToCompleted()
    {
        PlaybackEventStream stream;
        PlaybackSession session(stream);
        session.start(makeStream(QStringLiteral("S")),
            makeCtx(QStringLiteral("tt1")));
        session.markSourceResolved(domain::AssetRef {});
        session.markPlayableUrl(QStringLiteral("aid"),
            QUrl(QStringLiteral("http://x")));
        session.markPlayerLoaded();

        QSignalSpy spy(&stream, &PlaybackEventStream::eventPublished);
        session.markEnded(PlaybackEndReason::NaturalEof);

        QCOMPARE(session.state(), PlaybackState::Completed);
        const auto ended = extract<PlaybackEnded>(spy);
        QCOMPARE(ended.reason, PlaybackEndReason::NaturalEof);
        QCOMPARE(ended.sessionId, session.id());
        QVERIFY(session.isTerminal());
    }

    void markFailedPublishesPlaybackFailed()
    {
        PlaybackEventStream stream;
        PlaybackSession session(stream);
        session.start(makeStream(QStringLiteral("S")),
            makeCtx(QStringLiteral("tt1")));

        QSignalSpy spy(&stream, &PlaybackEventStream::eventPublished);
        session.markFailed(QStringLiteral("upstream error"));

        QCOMPARE(session.state(), PlaybackState::Failed);
        const auto failed = extract<PlaybackFailed>(spy);
        QCOMPARE(failed.reason, QStringLiteral("upstream error"));
        QCOMPARE(failed.sessionId, session.id());
    }

    void markReplacedByNewSourceTerminatesWithReplacedReason()
    {
        PlaybackEventStream stream;
        PlaybackSession session(stream);
        session.start(makeStream(QStringLiteral("S")),
            makeCtx(QStringLiteral("tt1")));

        QSignalSpy spy(&stream, &PlaybackEventStream::eventPublished);
        session.markReplacedByNewSource();

        const auto ended = extract<PlaybackEnded>(spy);
        QCOMPARE(ended.reason, PlaybackEndReason::ReplacedByNewSource);
        QVERIFY(session.isTerminal());

        // Idempotent.
        QSignalSpy spy2(&stream, &PlaybackEventStream::eventPublished);
        session.markReplacedByNewSource();
        QCOMPARE(spy2.count(), 0);
    }

    void differentSessionsHaveDifferentIds()
    {
        PlaybackEventStream stream;
        PlaybackSession a(stream);
        PlaybackSession b(stream);
        QVERIFY(a.id() != b.id());
    }
};

QTEST_MAIN(TstPlaybackSessionPlayFlow)
#include "tst_playback_session_play_flow.moc"
