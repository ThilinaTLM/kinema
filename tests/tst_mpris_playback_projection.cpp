// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/desktop/MprisPlaybackProjection.h"

#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "playback/events/PlaybackEvent.h"
#include "playback/events/PlaybackEventStream.h"
#include "playback/ports/PlayerPort.h"
#include "playback/session/PlaybackSessionManager.h"

#include <QSignalSpy>
#include <QString>
#include <QTest>
#include <QUuid>

#include <optional>
#include <utility>

using namespace kinema;
using namespace kinema::playback;
using namespace kinema::playback::desktop;
using namespace kinema::playback::events;
using namespace kinema::playback::session;
namespace ports = kinema::playback::ports;

namespace {

// Records every transport command instead of forwarding to an
// embedded controller; pause / resume / playPause / stop /
// seek* / setVolume / setRate are all `virtual` on the manager so
// the projection's commands can be observed without spinning up
// the real session machinery.
class RecordingSessionManager final : public PlaybackSessionManager
{
public:
    using PlaybackSessionManager::PlaybackSessionManager;

    void pause() override { ++pauseCalls; }
    void resume() override { ++resumeCalls; }
    void playPause() override { ++playPauseCalls; }
    void stop() override { ++stopCalls; }
    void seekRelativeSeconds(double s) override
    {
        seekRelativeCalls.append(s);
    }
    void seekAbsoluteSeconds(double s) override
    {
        seekAbsoluteCalls.append(s);
    }
    void setVolumePercent(double p) override
    {
        setVolumeCalls.append(p);
    }
    void setPlaybackRate(double f) override
    {
        setRateCalls.append(f);
    }

    int pauseCalls = 0;
    int resumeCalls = 0;
    int playPauseCalls = 0;
    int stopCalls = 0;
    QList<double> seekRelativeCalls;
    QList<double> seekAbsoluteCalls;
    QList<double> setVolumeCalls;
    QList<double> setRateCalls;
};

// In-memory PlayerPort that the projection queries for snapshot
// data (`Position` / `Volume` / `Rate`).
class StubPlayerPort final : public ports::PlayerPort
{
public:
    bool isAvailable() const override { return true; }
    void play(const QUrl&, const domain::PlaybackContext&,
        std::optional<qint64>) override {}
    void pause() override {}
    void resume() override {}
    void togglePause() override {}
    void stop() override {}
    void seekRelative(double) override {}
    void seekAbsolute(double) override {}
    void setVolumePercent(double) override {}
    void setPlaybackRate(double) override {}
    void selectAudioTrack(int) override {}
    void selectSubtitleTrack(int) override {}
    bool attachSubtitleFile(const QString&, const QString&) override
    {
        return false;
    }
    ports::PlayerSnapshot snapshot() const override { return snap; }

    ports::PlayerSnapshot snap;
};

domain::PlaybackContext makeMovieCtx(
    const QString& imdbId = QStringLiteral("tt1234567"))
{
    domain::PlaybackContext ctx;
    ctx.key.kind = domain::MediaKind::Movie;
    ctx.key.imdbId = imdbId;
    ctx.title = QStringLiteral("Arrival");
    return ctx;
}

PlaybackSessionId fresh()
{
    return QUuid::createUuid();
}

} // namespace

class TstMprisPlaybackProjection : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void init()
    {
        m_events = std::make_unique<PlaybackEventStream>();
        m_mgr = std::make_unique<RecordingSessionManager>(*m_events, nullptr, nullptr, nullptr);
        m_player = std::make_unique<StubPlayerPort>();
        m_projection = std::make_unique<MprisPlaybackProjection>(
            *m_events, *m_mgr, m_player.get(), /*series=*/nullptr);
    }

    void cleanup()
    {
        m_projection.reset();
        m_player.reset();
        m_mgr.reset();
        m_events.reset();
    }

    // -----------------------------------------------------------
    // Initial state: no session, Stopped, no Can* flags.
    // -----------------------------------------------------------
    void initialStateIsStopped()
    {
        QCOMPARE(m_projection->playbackStatus(),
            QStringLiteral("Stopped"));
        QVERIFY(m_projection->metadata().isEmpty());
        QVERIFY(!m_projection->hasActiveSession());
        QVERIFY(!m_projection->canPlay());
        QVERIFY(!m_projection->canPause());
        QVERIFY(!m_projection->canSeek());
        QVERIFY(!m_projection->canControl());
        QVERIFY(!m_projection->canGoNext());
        QVERIFY(!m_projection->canGoPrevious());
        QCOMPARE(m_projection->positionUs(), 0LL);
    }

    // -----------------------------------------------------------
    // PlaybackRequested makes the session active and Stopped
    // becomes Paused (no PlaybackStateChanged yet -> not Playing).
    // Metadata + Can* flags switch on.
    // -----------------------------------------------------------
    void playbackRequestedActivatesSession()
    {
        const auto sid = fresh();
        m_events->publish(PlaybackRequested { sid, makeMovieCtx() });

        QVERIFY(m_projection->hasActiveSession());
        QCOMPARE(m_projection->playbackStatus(),
            QStringLiteral("Paused"));
        QVERIFY(m_projection->canPlay());
        QVERIFY(m_projection->canPause());
        QVERIFY(m_projection->canSeek());
        QVERIFY(m_projection->canControl());
        QVERIFY(!m_projection->metadata().isEmpty());
    }

    // -----------------------------------------------------------
    // Playing state: PlaybackStateChanged(playing=true) flips
    // status to Playing.
    // -----------------------------------------------------------
    void playbackStateChangedDrivesStatus()
    {
        const auto sid = fresh();
        m_events->publish(PlaybackRequested { sid, makeMovieCtx() });
        m_events->publish(PlaybackStateChanged {
            sid, /*playing=*/true, /*paused=*/false,
            /*buffering=*/false });

        QCOMPARE(m_projection->playbackStatus(),
            QStringLiteral("Playing"));
        QVERIFY(m_projection->isActivelyPlaying());

        m_events->publish(PlaybackStateChanged {
            sid, /*playing=*/false, /*paused=*/true,
            /*buffering=*/false });

        QCOMPARE(m_projection->playbackStatus(),
            QStringLiteral("Paused"));
        QVERIFY(!m_projection->isActivelyPlaying());
    }

    // -----------------------------------------------------------
    // Stale events from a superseded session are ignored.
    // -----------------------------------------------------------
    void staleSessionEventsAreIgnored()
    {
        const auto sidA = fresh();
        const auto sidB = fresh();
        m_events->publish(PlaybackRequested { sidA, makeMovieCtx() });
        m_events->publish(PlaybackStateChanged {
            sidA, /*playing=*/true, /*paused=*/false, false });
        QCOMPARE(m_projection->playbackStatus(),
            QStringLiteral("Playing"));

        // New session arrives.
        m_events->publish(PlaybackRequested {
            sidB, makeMovieCtx(QStringLiteral("tt9999999")) });
        // Late PlaybackStateChanged from the prior session must
        // not flip status back to Playing under the new ctx.
        m_events->publish(PlaybackStateChanged {
            sidA, /*playing=*/true, /*paused=*/false, false });
        QCOMPARE(m_projection->playbackStatus(),
            QStringLiteral("Paused"));
    }

    // -----------------------------------------------------------
    // PlaybackEnded clears the session.
    // -----------------------------------------------------------
    void playbackEndedDeactivates()
    {
        const auto sid = fresh();
        const auto ctx = makeMovieCtx();
        m_events->publish(PlaybackRequested { sid, ctx });
        m_events->publish(PlaybackStateChanged {
            sid, /*playing=*/true, false, false });
        QVERIFY(m_projection->hasActiveSession());

        m_events->publish(PlaybackEnded {
            sid, PlaybackEndReason::NaturalEof, ctx });

        QVERIFY(!m_projection->hasActiveSession());
        QCOMPARE(m_projection->playbackStatus(),
            QStringLiteral("Stopped"));
        QVERIFY(m_projection->metadata().isEmpty());
    }

    // -----------------------------------------------------------
    // PlaybackFailed clears the session too.
    // -----------------------------------------------------------
    void playbackFailedDeactivates()
    {
        const auto sid = fresh();
        const auto ctx = makeMovieCtx();
        m_events->publish(PlaybackRequested { sid, ctx });
        m_events->publish(PlaybackFailed {
            sid, QStringLiteral("boom"), ctx });

        QVERIFY(!m_projection->hasActiveSession());
    }

    // -----------------------------------------------------------
    // Duration is cached and exposed via metadata.
    // -----------------------------------------------------------
    void durationFlowsIntoMetadata()
    {
        const auto sid = fresh();
        m_events->publish(PlaybackRequested { sid, makeMovieCtx() });
        m_events->publish(DurationChanged { sid, 1234.0 });

        const auto md = m_projection->metadata();
        QVERIFY(md.contains(QStringLiteral("mpris:length")));
        // mpris:length is in microseconds.
        QCOMPARE(md.value(QStringLiteral("mpris:length")).toLongLong(),
            1234000000LL);
    }

    // -----------------------------------------------------------
    // Snapshot from PlayerPort drives Position/Volume/Rate, even
    // for active sessions (we deliberately don't broadcast each
    // position tick).
    // -----------------------------------------------------------
    void positionVolumeRateUseSnapshot()
    {
        const auto sid = fresh();
        m_events->publish(PlaybackRequested { sid, makeMovieCtx() });

        m_player->snap.active = true;
        m_player->snap.positionSec = 42.5;
        m_player->snap.volumePercent = 75.0;
        m_player->snap.playbackRate = 1.5;

        QCOMPARE(m_projection->positionUs(), 42500000LL);
        QCOMPARE(m_projection->volume(), 0.75);
        QCOMPARE(m_projection->rate(), 1.5);
    }

    // -----------------------------------------------------------
    // Snapshot falls back to event-derived position when the port
    // reports no active session (defensive).
    // -----------------------------------------------------------
    void positionFallsBackToEventCacheWhenSnapshotInactive()
    {
        const auto sid = fresh();
        m_events->publish(PlaybackRequested { sid, makeMovieCtx() });
        m_events->publish(PositionTicked { sid, 7.0 });

        m_player->snap.active = false;
        m_player->snap.positionSec = 9999.0; // ignored

        QCOMPARE(m_projection->positionUs(), 7000000LL);
    }

    // -----------------------------------------------------------
    // Transport commands route through PlaybackSessionManager.
    // -----------------------------------------------------------
    void transportRoutesThroughSessionManager()
    {
        m_projection->pause();
        m_projection->play(); // Play == resume
        m_projection->playPause();
        m_projection->stop();
        m_projection->seek(2'500'000); // 2.5s in microseconds
        m_projection->setVolume(0.5);
        m_projection->setRate(2.0);

        QCOMPARE(m_mgr->pauseCalls, 1);
        QCOMPARE(m_mgr->resumeCalls, 1);
        QCOMPARE(m_mgr->playPauseCalls, 1);
        QCOMPARE(m_mgr->stopCalls, 1);
        QCOMPARE(m_mgr->seekRelativeCalls.size(), 1);
        QCOMPARE(m_mgr->seekRelativeCalls.first(), 2.5);
        QCOMPARE(m_mgr->setVolumeCalls.size(), 1);
        QCOMPARE(m_mgr->setVolumeCalls.first(), 50.0);
        QCOMPARE(m_mgr->setRateCalls.size(), 1);
        QCOMPARE(m_mgr->setRateCalls.first(), 2.0);
    }

    // -----------------------------------------------------------
    // SetPosition only routes when the trackId matches.
    // -----------------------------------------------------------
    void setPositionRespectsTrackId()
    {
        const auto sid = fresh();
        const auto ctx = makeMovieCtx();
        m_events->publish(PlaybackRequested { sid, ctx });

        const auto goodPath = m_projection->currentTrackObjectPath();
        QVERIFY(!goodPath.isEmpty());

        m_projection->setPosition(QStringLiteral("/wrong/path"),
            12'000'000);
        QCOMPARE(m_mgr->seekAbsoluteCalls.size(), 0);

        m_projection->setPosition(goodPath, 12'000'000);
        QCOMPARE(m_mgr->seekAbsoluteCalls.size(), 1);
        QCOMPARE(m_mgr->seekAbsoluteCalls.first(), 12.0);

        // Negative positions are dropped.
        m_projection->setPosition(goodPath, -1);
        QCOMPARE(m_mgr->seekAbsoluteCalls.size(), 1);
    }

    // -----------------------------------------------------------
    // SetRate of zero or negative is ignored.
    // -----------------------------------------------------------
    void setRateRejectsNonPositive()
    {
        m_projection->setRate(0.0);
        m_projection->setRate(-1.5);
        m_projection->setRate(2.0);
        QCOMPARE(m_mgr->setRateCalls.size(), 1);
        QCOMPARE(m_mgr->setRateCalls.first(), 2.0);
    }

    // -----------------------------------------------------------
    // Raise / Quit re-emit through the projection.
    // -----------------------------------------------------------
    void raiseAndQuitForwardAsSignals()
    {
        QSignalSpy raiseSpy(m_projection.get(),
            &MprisPlaybackProjection::raiseRequested);
        QSignalSpy quitSpy(m_projection.get(),
            &MprisPlaybackProjection::quitRequested);

        m_projection->raise();
        m_projection->quit();

        QCOMPARE(raiseSpy.size(), 1);
        QCOMPARE(quitSpy.size(), 1);
    }

    // -----------------------------------------------------------
    // Without a series service, Next/Previous are no-ops and
    // CanGoNext / CanGoPrevious are false.
    // -----------------------------------------------------------
    void nextPreviousNoopWithoutSeries()
    {
        QVERIFY(!m_projection->canGoNext());
        QVERIFY(!m_projection->canGoPrevious());

        m_projection->next();
        m_projection->previous();
        // No assertions on m_mgr — manager's play() is virtual but
        // we route Next/Previous through SeriesSessionService, not
        // the manager directly.
    }

    // -----------------------------------------------------------
    // The desktop-entry / identity / supported-schemes are stable
    // constants the desktop file relies on.
    // -----------------------------------------------------------
    void rootIdentityIsStable()
    {
        QCOMPARE(m_projection->identity(),
            QStringLiteral("Kinema"));
        QCOMPARE(m_projection->desktopEntry(),
            QStringLiteral("dev.tlmtech.kinema"));
        const auto schemes = m_projection->supportedUriSchemes();
        QVERIFY(schemes.contains(QStringLiteral("file")));
        QVERIFY(schemes.contains(QStringLiteral("http")));
        QVERIFY(schemes.contains(QStringLiteral("https")));
    }

private:
        std::unique_ptr<PlaybackEventStream> m_events;
    std::unique_ptr<RecordingSessionManager> m_mgr;
    std::unique_ptr<StubPlayerPort> m_player;
    std::unique_ptr<MprisPlaybackProjection> m_projection;
};

QTEST_MAIN(TstMprisPlaybackProjection)
#include "tst_mpris_playback_projection.moc"
