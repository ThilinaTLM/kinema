// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/adapters/EmbeddedMpvPlayerAdapter.h"

#include "config/PlayerSettings.h"
#include "playback/events/PlaybackEvent.h"
#include "playback/events/PlaybackEventStream.h"

#include <KConfig>
#include <KSharedConfig>

#include <QSignalSpy>
#include <QStandardPaths>
#include <QTest>
#include <QUuid>

#include <chrono>
#include <variant>

using namespace kinema;
using namespace kinema::playback;
using namespace kinema::playback::adapters;
using namespace kinema::playback::events;
using namespace std::chrono_literals;

namespace {

domain::PlaybackContext makeCtx(const QString& id)
{
    domain::PlaybackContext ctx;
    domain::PlaybackKey key;
    key.kind = domain::MediaKind::Movie;
    key.imdbId = id;
    ctx.key = key;
    ctx.title = id;
    return ctx;
}

// One scratch PlayerSettings shared across every test in the file.
// The adapter only reads `resumePromptThresholdSec()` and
// `skipIntroChapters()`; defaults are fine for the existing
// assertions.
config::PlayerSettings& playerSettings()
{
    static KSharedConfigPtr cfg = KSharedConfig::openConfig(
        QStringLiteral("kinema-test-embedded-adapter"),
        KConfig::SimpleConfig, QStandardPaths::TempLocation);
    static config::PlayerSettings settings(cfg);
    return settings;
}

template<class T>
bool spyHasEvent(const QSignalSpy& spy)
{
    for (const auto& row : spy) {
        const auto event = row.first().value<PlaybackEvent>();
        if (std::holds_alternative<T>(event)) {
            return true;
        }
    }
    return false;
}

template<class T>
T extractEvent(const QSignalSpy& spy)
{
    for (const auto& row : spy) {
        const auto event = row.first().value<PlaybackEvent>();
        if (std::holds_alternative<T>(event)) {
            return std::get<T>(event);
        }
    }
    return T {};
}

} // namespace

class TstEmbeddedMpvPlayerAdapter : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void isUnavailableWithoutWindow()
    {
        PlaybackEventStream stream;
        EmbeddedMpvPlayerAdapter adapter(stream, playerSettings());

        QVERIFY(!adapter.isAvailable());
        QCOMPARE(adapter.snapshot().active, false);
    }

    void playWithoutWindowPublishesFailure()
    {
        PlaybackEventStream stream;
        EmbeddedMpvPlayerAdapter adapter(stream, playerSettings());

        const auto id = QUuid::createUuid();
        const auto ctx = makeCtx(QStringLiteral("tt1"));
        adapter.setActiveSession(id, ctx);

        QSignalSpy spy(&stream, &PlaybackEventStream::eventPublished);
        adapter.play(QUrl(QStringLiteral("http://example/x.mkv")), ctx);

        QVERIFY(spyHasEvent<PlaybackFailed>(spy));
        QVERIFY(!adapter.loadWatchdog().isActive());
    }

    void stopPublishesUserStopAndClearsSession()
    {
        PlaybackEventStream stream;
        EmbeddedMpvPlayerAdapter adapter(stream, playerSettings());

        const auto id = QUuid::createUuid();
        adapter.setActiveSession(id, makeCtx(QStringLiteral("tt1")));

        QSignalSpy spy(&stream, &PlaybackEventStream::eventPublished);
        adapter.stop();

        QVERIFY(spyHasEvent<PlaybackEnded>(spy));
        const auto ended = extractEvent<PlaybackEnded>(spy);
        QCOMPARE(ended.reason, PlaybackEndReason::UserStop);
        QCOMPARE(ended.sessionId, id);

        // Subsequent stop is a no-op.
        QSignalSpy spy2(&stream, &PlaybackEventStream::eventPublished);
        adapter.stop();
        QCOMPARE(spy2.count(), 0);
    }

    void fileLoadedPublishesPlayerLoadedAndDisarmsWatchdog()
    {
        PlaybackEventStream stream;
        EmbeddedMpvPlayerAdapter adapter(stream, playerSettings());

        const auto id = QUuid::createUuid();
        adapter.setActiveSession(id, makeCtx(QStringLiteral("tt1")));
        adapter.loadWatchdog().setTimeout(5s);
        adapter.loadWatchdog().start();
        QVERIFY(adapter.loadWatchdog().isActive());

        QSignalSpy spy(&stream, &PlaybackEventStream::eventPublished);
        QVERIFY(QMetaObject::invokeMethod(&adapter, "onFileLoaded"));

        QVERIFY(spyHasEvent<PlayerLoaded>(spy));
        QVERIFY(!adapter.loadWatchdog().isActive());
    }

    void naturalEofPublishesPlaybackEnded()
    {
        PlaybackEventStream stream;
        EmbeddedMpvPlayerAdapter adapter(stream, playerSettings());
        const auto id = QUuid::createUuid();
        const auto ctx = makeCtx(QStringLiteral("tt1"));
        adapter.setActiveSession(id, ctx);

        QSignalSpy spy(&stream, &PlaybackEventStream::eventPublished);
        QVERIFY(QMetaObject::invokeMethod(&adapter, "onEndOfFile",
            Q_ARG(QString, QStringLiteral("eof"))));

        const auto ended = extractEvent<PlaybackEnded>(spy);
        QCOMPARE(ended.reason, PlaybackEndReason::NaturalEof);
        QCOMPARE(ended.sessionId, id);
    }

    void mpvErrorPublishesPlaybackFailed()
    {
        PlaybackEventStream stream;
        EmbeddedMpvPlayerAdapter adapter(stream, playerSettings());
        const auto id = QUuid::createUuid();
        adapter.setActiveSession(id, makeCtx(QStringLiteral("tt1")));

        QSignalSpy spy(&stream, &PlaybackEventStream::eventPublished);
        QVERIFY(QMetaObject::invokeMethod(&adapter, "onMpvError",
            Q_ARG(QString, QStringLiteral("network error"))));

        const auto failed = extractEvent<PlaybackFailed>(spy);
        QCOMPARE(failed.sessionId, id);
        QCOMPARE(failed.reason, QStringLiteral("network error"));
    }

    void replaceBySessionFiltersStopEndFile()
    {
        // Simulate: session A active; session B then claimed; mpv
        // delivers a stale `stop` end-file for A *after* B was set
        // active. Expected: no spurious PlaybackEnded — the
        // suppressing m_loadfileInFlight flag must be respected.
        PlaybackEventStream stream;
        EmbeddedMpvPlayerAdapter adapter(stream, playerSettings());

        const auto idA = QUuid::createUuid();
        adapter.setActiveSession(idA, makeCtx(QStringLiteral("ttA")));

        const auto idB = QUuid::createUuid();
        adapter.setActiveSession(idB, makeCtx(QStringLiteral("ttB")));

        QSignalSpy spy(&stream, &PlaybackEventStream::eventPublished);
        QVERIFY(QMetaObject::invokeMethod(&adapter, "onEndOfFile",
            Q_ARG(QString, QStringLiteral("stop"))));

        // The stale "stop" should be filtered: no PlaybackEnded.
        QVERIFY(!spyHasEvent<PlaybackEnded>(spy));

        // Once mpv reports fileLoaded for B, the flag clears and a
        // subsequent natural EOF for B is published normally.
        QVERIFY(QMetaObject::invokeMethod(&adapter, "onFileLoaded"));
        QSignalSpy spy2(&stream, &PlaybackEventStream::eventPublished);
        QVERIFY(QMetaObject::invokeMethod(&adapter, "onEndOfFile",
            Q_ARG(QString, QStringLiteral("eof"))));
        const auto ended = extractEvent<PlaybackEnded>(spy2);
        QCOMPARE(ended.reason, PlaybackEndReason::NaturalEof);
        QCOMPARE(ended.sessionId, idB);
    }

    void positionAndDurationFanOutToEvents()
    {
        PlaybackEventStream stream;
        EmbeddedMpvPlayerAdapter adapter(stream, playerSettings());
        const auto id = QUuid::createUuid();
        adapter.setActiveSession(id, makeCtx(QStringLiteral("tt1")));

        QSignalSpy spy(&stream, &PlaybackEventStream::eventPublished);
        QVERIFY(QMetaObject::invokeMethod(&adapter, "onPositionChanged",
            Q_ARG(double, 42.5)));
        QVERIFY(QMetaObject::invokeMethod(&adapter, "onDurationChanged",
            Q_ARG(double, 7200.0)));

        const auto pos = extractEvent<PositionTicked>(spy);
        QCOMPARE(pos.seconds, 42.5);
        const auto dur = extractEvent<DurationChanged>(spy);
        QCOMPARE(dur.seconds, 7200.0);

        const auto snap = adapter.snapshot();
        QCOMPARE(snap.positionSec, 42.5);
        QCOMPARE(snap.durationSec, 7200.0);
    }

    void watchdogTimeoutPublishesLoadTimeout()
    {
        PlaybackEventStream stream;
        EmbeddedMpvPlayerAdapter adapter(stream, playerSettings());
        const auto id = QUuid::createUuid();
        adapter.setActiveSession(id, makeCtx(QStringLiteral("tt1")));

        adapter.loadWatchdog().setTimeout(20ms);

        QSignalSpy spy(&stream, &PlaybackEventStream::eventPublished);
        adapter.loadWatchdog().start();

        // Wait for the watchdog to fire and the adapter to publish.
        QTRY_VERIFY_WITH_TIMEOUT(spyHasEvent<PlaybackEnded>(spy), 500);
        const auto ended = extractEvent<PlaybackEnded>(spy);
        QCOMPARE(ended.reason, PlaybackEndReason::LoadTimeout);
        QCOMPARE(ended.sessionId, id);
    }

    void userClosedWindowPublishesUserStop()
    {
        PlaybackEventStream stream;
        EmbeddedMpvPlayerAdapter adapter(stream, playerSettings());
        const auto id = QUuid::createUuid();
        adapter.setActiveSession(id, makeCtx(QStringLiteral("tt1")));

        QSignalSpy spy(&stream, &PlaybackEventStream::eventPublished);
        QVERIFY(QMetaObject::invokeMethod(&adapter, "onUserClosedWindow"));

        const auto ended = extractEvent<PlaybackEnded>(spy);
        QCOMPARE(ended.reason, PlaybackEndReason::UserStop);
        QCOMPARE(ended.sessionId, id);
    }
};

QTEST_MAIN(TstEmbeddedMpvPlayerAdapter)
#include "tst_embedded_mpv_player_adapter.moc"
