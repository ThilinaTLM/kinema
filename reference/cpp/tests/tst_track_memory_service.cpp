// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/history/TrackMemoryService.h"

#include "core/mpv/MpvTrackList.h"
#include "core/persistence/Database.h"
#include "core/persistence/HistoryStore.h"
#include "domain/PlaybackContext.h"
#include "playback/events/PlaybackEvent.h"
#include "playback/events/PlaybackEventStream.h"
#include "playback/history/HistoryQueryService.h"
#include "playback/ports/PlaybackHistoryRepository.h"
#include "playback/ports/PlayerPort.h"

#include <QTest>
#include <QUuid>

#include <memory>
#include <optional>

using namespace kinema;
using namespace kinema::playback;
using namespace kinema::playback::events;
using namespace kinema::playback::history;
namespace ports = kinema::playback::ports;

namespace {

class StubHistoryRepository final
    : public ports::PlaybackHistoryRepository
{
public:
    void record(const domain::HistoryEntry&) override {}
    void recordSessionEnd(const domain::HistoryEntry&,
        PlaybackEndReason,
        std::optional<double>) override {}
    void remove(const domain::PlaybackKey&) override {}
    std::optional<domain::HistoryEntry> find(
        const domain::PlaybackKey&) const override { return std::nullopt; }
    std::optional<domain::HistoryEntry> findLatestForMedia(
        domain::MediaKind, const QString&) const override
    {
        return std::nullopt;
    }
    QList<domain::HistoryEntry> continueWatching(int) const override
    {
        return {};
    }
};

class FakeHistoryQueryService final : public HistoryQueryService
{
public:
    FakeHistoryQueryService(
        ports::PlaybackHistoryRepository& repo,
        core::HistoryStore& store, QObject* parent = nullptr)
        : HistoryQueryService(repo, store, parent)
    {
    }

    std::optional<domain::HistoryEntry> find(
        const domain::PlaybackKey&) const override
    {
        return stored;
    }

    std::optional<domain::HistoryEntry> stored;
};

class RecordingPlayerPort final : public ports::PlayerPort
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
    void selectAudioTrack(int id) override { audioCalls.append(id); }
    void selectSubtitleTrack(int id) override { subtitleCalls.append(id); }
    bool attachSubtitleFile(const QString&, const QString&) override
    {
        return false;
    }
    ports::PlayerSnapshot snapshot() const override { return {}; }

    QList<int> audioCalls;
    QList<int> subtitleCalls;
};

core::tracks::Entry makeEntry(int id, const QString& type,
    const QString& lang, bool selected)
{
    core::tracks::Entry e;
    e.id = id;
    e.type = type;
    e.lang = lang;
    e.selected = selected;
    return e;
}

core::tracks::TrackList trackList()
{
    return {
        makeEntry(1, QStringLiteral("audio"), QStringLiteral("eng"), true),
        makeEntry(2, QStringLiteral("audio"), QStringLiteral("jpn"), false),
        makeEntry(3, QStringLiteral("sub"), QStringLiteral("eng"), true),
        makeEntry(4, QStringLiteral("sub"), QStringLiteral("fre"), false),
    };
}

domain::PlaybackContext makeCtx()
{
    domain::PlaybackContext ctx;
    ctx.key.kind = domain::MediaKind::Movie;
    ctx.key.imdbId = QStringLiteral("tt1234567");
    ctx.title = QStringLiteral("Movie");
    return ctx;
}

PlaybackSessionId fresh()
{
    return QUuid::createUuid();
}

} // namespace

class TstTrackMemoryService : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void init()
    {
        m_repo = std::make_unique<StubHistoryRepository>();
        // `HistoryQueryService` connects to `HistoryStore::changed`
        // for the `changed()` re-broadcast. The fake's `find()`
        // override is what the service-under-test actually reads;
        // the in-memory database here is only standing the store
        // up so the connect call has a target.
        m_db = std::make_unique<core::Database>(
            QStringLiteral(":memory:"), nullptr);
        QVERIFY(m_db->open());
        m_store = std::make_unique<core::HistoryStore>(*m_db);
        m_history = std::make_unique<FakeHistoryQueryService>(
            *m_repo, *m_store);
        m_events = std::make_unique<PlaybackEventStream>();
        m_player = std::make_unique<RecordingPlayerPort>();
        m_svc = std::make_unique<TrackMemoryService>(*m_events,
            *m_history, m_player.get());
    }

    void cleanup()
    {
        m_svc.reset();
        m_player.reset();
        m_events.reset();
        m_history.reset();
        m_store.reset();
        m_db.reset();
        m_repo.reset();
    }

    // -----------------------------------------------------------
    // No stored history -> applier latches but issues no track
    // selection calls.
    // -----------------------------------------------------------
    void noStoredHistoryIsNoop()
    {
        m_history->stored = std::nullopt;
        const auto sid = fresh();
        m_events->publish(PlaybackRequested { sid, makeCtx() });
        m_events->publish(TrackListChanged { sid, trackList() });

        QCOMPARE(m_player->audioCalls.size(), 0);
        QCOMPARE(m_player->subtitleCalls.size(), 0);
    }

    // -----------------------------------------------------------
    // Remembered audio language different from currently selected
    // -> selectAudioTrack(<remembered id>) once.
    // -----------------------------------------------------------
    void remembersAudioLanguage()
    {
        domain::HistoryEntry entry;
        entry.key.imdbId = QStringLiteral("tt1234567");
        entry.rememberedAudioLang = QStringLiteral("jpn");
        m_history->stored = entry;

        const auto sid = fresh();
        m_events->publish(PlaybackRequested { sid, makeCtx() });
        m_events->publish(TrackListChanged { sid, trackList() });

        QCOMPARE(m_player->audioCalls.size(), 1);
        QCOMPARE(m_player->audioCalls.first(), 2);
    }

    // -----------------------------------------------------------
    // Remembered audio matches current selection -> no-op.
    // -----------------------------------------------------------
    void noOpWhenAudioAlreadyMatches()
    {
        domain::HistoryEntry entry;
        entry.rememberedAudioLang = QStringLiteral("eng");
        m_history->stored = entry;

        const auto sid = fresh();
        m_events->publish(PlaybackRequested { sid, makeCtx() });
        m_events->publish(TrackListChanged { sid, trackList() });

        QCOMPARE(m_player->audioCalls.size(), 0);
    }

    // -----------------------------------------------------------
    // Subtitle preference `off` deselects any selected sub track.
    // -----------------------------------------------------------
    void remembersSubtitleOff()
    {
        domain::HistoryEntry entry;
        entry.rememberedSubtitleLang = QStringLiteral("off");
        m_history->stored = entry;

        const auto sid = fresh();
        m_events->publish(PlaybackRequested { sid, makeCtx() });
        m_events->publish(TrackListChanged { sid, trackList() });

        QCOMPARE(m_player->subtitleCalls.size(), 1);
        QCOMPARE(m_player->subtitleCalls.first(), -1);
    }

    // -----------------------------------------------------------
    // Remembered subtitle language switches the active track.
    // -----------------------------------------------------------
    void remembersSubtitleLanguage()
    {
        domain::HistoryEntry entry;
        entry.rememberedSubtitleLang = QStringLiteral("fre");
        m_history->stored = entry;

        const auto sid = fresh();
        m_events->publish(PlaybackRequested { sid, makeCtx() });
        m_events->publish(TrackListChanged { sid, trackList() });

        QCOMPARE(m_player->subtitleCalls.size(), 1);
        QCOMPARE(m_player->subtitleCalls.first(), 4);
    }

    // -----------------------------------------------------------
    // The applier runs at most once per session: a second
    // TrackListChanged is ignored.
    // -----------------------------------------------------------
    void appliesAtMostOncePerSession()
    {
        domain::HistoryEntry entry;
        entry.rememberedAudioLang = QStringLiteral("jpn");
        m_history->stored = entry;

        const auto sid = fresh();
        m_events->publish(PlaybackRequested { sid, makeCtx() });
        m_events->publish(TrackListChanged { sid, trackList() });
        m_events->publish(TrackListChanged { sid, trackList() });

        QCOMPARE(m_player->audioCalls.size(), 1);
    }

    // -----------------------------------------------------------
    // A fresh PlaybackRequested resets the applier; the next
    // TrackListChanged fires again.
    // -----------------------------------------------------------
    void freshSessionResetsApplier()
    {
        domain::HistoryEntry entry;
        entry.rememberedAudioLang = QStringLiteral("jpn");
        m_history->stored = entry;

        const auto sid1 = fresh();
        m_events->publish(PlaybackRequested { sid1, makeCtx() });
        m_events->publish(TrackListChanged { sid1, trackList() });
        QCOMPARE(m_player->audioCalls.size(), 1);

        const auto sid2 = fresh();
        m_events->publish(PlaybackRequested { sid2, makeCtx() });
        m_events->publish(TrackListChanged { sid2, trackList() });
        QCOMPARE(m_player->audioCalls.size(), 2);
    }

    // -----------------------------------------------------------
    // Stale events from a superseded session don't fire selection.
    // -----------------------------------------------------------
    void staleSessionEventsAreIgnored()
    {
        domain::HistoryEntry entry;
        entry.rememberedAudioLang = QStringLiteral("jpn");
        m_history->stored = entry;

        const auto sid1 = fresh();
        const auto sid2 = fresh();
        m_events->publish(PlaybackRequested { sid1, makeCtx() });
        m_events->publish(PlaybackRequested { sid2, makeCtx() });
        // Late TrackListChanged from prior session -> dropped.
        m_events->publish(TrackListChanged { sid1, trackList() });

        QCOMPARE(m_player->audioCalls.size(), 0);
    }

private:
    std::unique_ptr<StubHistoryRepository> m_repo;
    std::unique_ptr<core::Database> m_db;
    std::unique_ptr<core::HistoryStore> m_store;
    std::unique_ptr<FakeHistoryQueryService> m_history;
    std::unique_ptr<PlaybackEventStream> m_events;
    std::unique_ptr<RecordingPlayerPort> m_player;
    std::unique_ptr<TrackMemoryService> m_svc;
};

QTEST_MAIN(TstTrackMemoryService)
#include "tst_track_memory_service.moc"
