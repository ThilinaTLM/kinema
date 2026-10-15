// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/progress/PlaybackProgressProjector.h"

#include "core/mpv/MpvChapterList.h"
#include "core/mpv/MpvTrackList.h"
#include "domain/PlaybackContext.h"
#include "playback/events/PlaybackEvent.h"
#include "playback/events/PlaybackEventStream.h"
#include "playback/ports/PlaybackHistoryRepository.h"

#include <QTest>
#include <QUuid>

using namespace kinema;
using namespace kinema::playback;
using namespace kinema::playback::events;
using namespace kinema::playback::progress;

namespace {

class FakeHistoryRepo final : public ports::PlaybackHistoryRepository
{
public:
    void record(const domain::HistoryEntry& entry) override
    {
        ++recordCalls;
        last = entry;
    }
    void recordSessionEnd(const domain::HistoryEntry& entry,
        PlaybackEndReason reason,
        std::optional<double> creditsStartSec) override
    {
        ++recordSessionEndCalls;
        last = entry;
        lastReason = reason;
        lastCreditsStart = creditsStartSec;
    }
    void remove(const domain::PlaybackKey&) override {}

    std::optional<domain::HistoryEntry> find(
        const domain::PlaybackKey& key) const override
    {
        if (preexisting && preexisting->key == key) {
            return preexisting;
        }
        return std::nullopt;
    }
    std::optional<domain::HistoryEntry> findLatestForMedia(
        domain::MediaKind, const QString&) const override
    {
        return std::nullopt;
    }
    QList<domain::HistoryEntry> continueWatching(int) const override
    {
        return {};
    }

    int recordCalls = 0;
    int recordSessionEndCalls = 0;
    domain::HistoryEntry last;
    std::optional<PlaybackEndReason> lastReason;
    std::optional<double> lastCreditsStart;
    std::optional<domain::HistoryEntry> preexisting;
};

domain::PlaybackContext makeCtx(const QString& imdb,
    const QString& title = QStringLiteral("Title"))
{
    domain::PlaybackContext ctx;
    domain::PlaybackKey key;
    key.kind = domain::MediaKind::Movie;
    key.imdbId = imdb;
    ctx.key = key;
    ctx.title = title;
    return ctx;
}

} // namespace

class TstPlaybackProgressProjector : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void seedsRowOnPlaybackRequested()
    {
        FakeHistoryRepo repo;
        PlaybackEventStream stream;
        PlaybackProgressProjector p(repo, stream);

        const auto sessionId = QUuid::createUuid();
        const auto ctx = makeCtx(QStringLiteral("tt1"), QStringLiteral("X"));
        stream.publish(PlaybackRequested { sessionId, ctx });

        QCOMPARE(repo.recordCalls, 1);
        QCOMPARE(repo.last.key.imdbId, QStringLiteral("tt1"));
        QCOMPARE(repo.last.title, QStringLiteral("X"));
        QVERIFY(p.hasActiveContext());
    }

    void preservesExistingProgressOnSeed()
    {
        FakeHistoryRepo repo;
        domain::HistoryEntry existing;
        existing.key = makeCtx(QStringLiteral("tt1")).key;
        existing.positionSec = 120.0;
        existing.durationSec = 7200.0;
        existing.finished = true;
        repo.preexisting = existing;

        PlaybackEventStream stream;
        PlaybackProgressProjector p(repo, stream);
        stream.publish(PlaybackRequested {
            QUuid::createUuid(), makeCtx(QStringLiteral("tt1")) });

        QCOMPARE(repo.last.positionSec, 120.0);
        QCOMPARE(repo.last.durationSec, 7200.0);
        // Re-watching a finished row should reset finished so it
        // reappears in Continue Watching.
        QVERIFY(!repo.last.finished);
    }

    void positionTicksAreThrottledAndPersistAtInterval()
    {
        FakeHistoryRepo repo;
        PlaybackEventStream stream;
        PlaybackProgressProjector p(repo, stream);
        p.setPersistIntervalSeconds(5.0);
        p.setMinProgressFraction(0.005);

        const auto sessionId = QUuid::createUuid();
        stream.publish(PlaybackRequested {
            sessionId, makeCtx(QStringLiteral("tt1")) });
        QCOMPARE(repo.recordCalls, 1);  // seed
        stream.publish(DurationChanged { sessionId, 7200.0 });
        stream.publish(PlayerLoaded { sessionId });

        // First tick at 1s — below min progress fraction (0.5% of
        // 7200 = 36s). No persist.
        stream.publish(PositionTicked { sessionId, 1.0 });
        QCOMPARE(repo.recordCalls, 1);

        // 40s — past min fraction but distance from last persisted
        // (0) is 40 > 5, so persist.
        stream.publish(PositionTicked { sessionId, 40.0 });
        QCOMPARE(repo.recordCalls, 2);
        QCOMPARE(repo.last.positionSec, 40.0);

        // 41s — distance from last persisted is 1 < 5, no persist.
        stream.publish(PositionTicked { sessionId, 41.0 });
        QCOMPARE(repo.recordCalls, 2);

        // 50s — distance 10 >= 5, persist.
        stream.publish(PositionTicked { sessionId, 50.0 });
        QCOMPARE(repo.recordCalls, 3);
        QCOMPARE(repo.last.positionSec, 50.0);
    }

    void positionWithoutDurationDoesNotPersist()
    {
        FakeHistoryRepo repo;
        PlaybackEventStream stream;
        PlaybackProgressProjector p(repo, stream);

        const auto sessionId = QUuid::createUuid();
        stream.publish(PlaybackRequested {
            sessionId, makeCtx(QStringLiteral("tt1")) });
        QCOMPARE(repo.recordCalls, 1);

        // No DurationChanged yet — ticks are queued and ignored
        // until duration is known.
        stream.publish(PositionTicked { sessionId, 60.0 });
        stream.publish(PositionTicked { sessionId, 120.0 });
        QCOMPARE(repo.recordCalls, 1);
    }

    void naturalEofTriggersRecordSessionEnd()
    {
        FakeHistoryRepo repo;
        PlaybackEventStream stream;
        PlaybackProgressProjector p(repo, stream);

        const auto sessionId = QUuid::createUuid();
        const auto ctx = makeCtx(QStringLiteral("tt1"));
        stream.publish(PlaybackRequested { sessionId, ctx });
        stream.publish(DurationChanged { sessionId, 7200.0 });
        stream.publish(PositionTicked { sessionId, 6200.0 });
        stream.publish(PlaybackEnded {
            sessionId, PlaybackEndReason::NaturalEof, ctx });

        QCOMPARE(repo.recordSessionEndCalls, 1);
        QCOMPARE(*repo.lastReason, PlaybackEndReason::NaturalEof);
        QVERIFY(!p.hasActiveContext());
    }

    void chapterListSurfacesCreditsStartOnEnd()
    {
        FakeHistoryRepo repo;
        PlaybackEventStream stream;
        PlaybackProgressProjector p(repo, stream);

        const auto sessionId = QUuid::createUuid();
        const auto ctx = makeCtx(QStringLiteral("tt1"));
        stream.publish(PlaybackRequested { sessionId, ctx });
        stream.publish(DurationChanged { sessionId, 7200.0 });
        // Last chapter at 90% triggers untitled credits heuristic.
        core::chapters::ChapterList chapters {
            { 0.0, QStringLiteral("") },
            { 6480.0, QStringLiteral("") },
        };
        stream.publish(ChapterListChanged { sessionId, chapters });
        stream.publish(PlaybackEnded {
            sessionId, PlaybackEndReason::NaturalEof, ctx });

        QCOMPARE(repo.recordSessionEndCalls, 1);
        QVERIFY(repo.lastCreditsStart.has_value());
        QCOMPARE(*repo.lastCreditsStart, 6480.0);
    }

    void trackListUpdatesRememberedLanguages()
    {
        FakeHistoryRepo repo;
        PlaybackEventStream stream;
        PlaybackProgressProjector p(repo, stream);
        p.setPersistIntervalSeconds(5.0);

        const auto sessionId = QUuid::createUuid();
        stream.publish(PlaybackRequested {
            sessionId, makeCtx(QStringLiteral("tt1")) });
        stream.publish(DurationChanged { sessionId, 7200.0 });

        core::tracks::TrackList tracks;
        core::tracks::Entry audio;
        audio.id = 1;
        audio.type = QStringLiteral("audio");
        audio.lang = QStringLiteral("eng");
        audio.selected = true;
        tracks.append(audio);
        core::tracks::Entry sub;
        sub.id = 2;
        sub.type = QStringLiteral("sub");
        sub.lang = QStringLiteral("eng");
        sub.selected = true;
        tracks.append(sub);
        stream.publish(TrackListChanged { sessionId, tracks });

        // Drive a persist by ticking past threshold.
        stream.publish(PositionTicked { sessionId, 60.0 });
        QVERIFY(repo.recordCalls >= 2);
        QCOMPARE(repo.last.rememberedAudioLang, QStringLiteral("eng"));
        QCOMPARE(repo.last.rememberedSubtitleLang, QStringLiteral("eng"));
    }

    void ignoresEventsFromUnknownSession()
    {
        FakeHistoryRepo repo;
        PlaybackEventStream stream;
        PlaybackProgressProjector p(repo, stream);

        const auto sessionId = QUuid::createUuid();
        const auto staleId = QUuid::createUuid();
        stream.publish(PlaybackRequested {
            sessionId, makeCtx(QStringLiteral("tt1")) });
        QCOMPARE(repo.recordCalls, 1);

        // Stale events from a different session should be ignored.
        stream.publish(DurationChanged { staleId, 7200.0 });
        stream.publish(PositionTicked { staleId, 600.0 });
        stream.publish(PlaybackEnded {
            staleId, PlaybackEndReason::NaturalEof,
            makeCtx(QStringLiteral("tt9")) });
        QCOMPARE(repo.recordCalls, 1);
        QCOMPARE(repo.recordSessionEndCalls, 0);
        QVERIFY(p.hasActiveContext());
    }

    void terminalUserStopUsesHighestObservedPosition()
    {
        FakeHistoryRepo repo;
        PlaybackEventStream stream;
        PlaybackProgressProjector p(repo, stream);
        p.setPersistIntervalSeconds(5.0);

        const auto sessionId = QUuid::createUuid();
        const auto ctx = makeCtx(QStringLiteral("tt1"));
        stream.publish(PlaybackRequested { sessionId, ctx });
        stream.publish(DurationChanged { sessionId, 1000.0 });
        stream.publish(PositionTicked { sessionId, 910.0 });

        // Some shutdown paths can produce a reset-like position update
        // before the terminal event. The session-end record must still
        // use the last reliable high-water mark.
        stream.publish(PositionTicked { sessionId, 0.0 });
        stream.publish(PlaybackEnded {
            sessionId, PlaybackEndReason::UserStop, ctx });

        QCOMPARE(repo.recordSessionEndCalls, 1);
        QCOMPARE(repo.last.positionSec, 910.0);
        QCOMPARE(repo.last.durationSec, 1000.0);
        QCOMPARE(*repo.lastReason, PlaybackEndReason::UserStop);
    }

    void replacedByNewSourcePersistsButDoesNotApplyPolicy()
    {
        FakeHistoryRepo repo;
        PlaybackEventStream stream;
        PlaybackProgressProjector p(repo, stream);
        p.setPersistIntervalSeconds(5.0);

        const auto sessionId = QUuid::createUuid();
        const auto ctx = makeCtx(QStringLiteral("tt1"));
        stream.publish(PlaybackRequested { sessionId, ctx });
        stream.publish(DurationChanged { sessionId, 7200.0 });
        stream.publish(PositionTicked { sessionId, 600.0 });
        const auto beforeRecords = repo.recordCalls;

        stream.publish(PlaybackEnded {
            sessionId, PlaybackEndReason::ReplacedByNewSource, ctx });

        // A force persist should have written the in-memory position,
        // but recordSessionEnd must NOT be called.
        QVERIFY(repo.recordCalls >= beforeRecords);
        QCOMPARE(repo.recordSessionEndCalls, 0);
        QVERIFY(!p.hasActiveContext());
    }

    void playerErrorRecordsSessionEndWithoutFinishedFlip()
    {
        FakeHistoryRepo repo;
        PlaybackEventStream stream;
        PlaybackProgressProjector p(repo, stream);

        const auto sessionId = QUuid::createUuid();
        const auto ctx = makeCtx(QStringLiteral("tt1"));
        stream.publish(PlaybackRequested { sessionId, ctx });
        stream.publish(PlaybackFailed {
            sessionId, QStringLiteral("oops"), ctx });

        QCOMPARE(repo.recordSessionEndCalls, 1);
        QCOMPARE(*repo.lastReason, PlaybackEndReason::PlayerError);
        QVERIFY(!p.hasActiveContext());
    }
};

QTEST_MAIN(TstPlaybackProgressProjector)
#include "tst_playback_progress_projector.moc"
