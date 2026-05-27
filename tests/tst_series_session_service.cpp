// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "domain/Media.h"
#include "domain/MediaFile.h"
#include "domain/PlaybackContext.h"
#include "playback/events/PlaybackEventStream.h"
#include "playback/ports/SessionFileCatalog.h"
#include "playback/series/SeriesSessionService.h"
#include "playback/session/PlaybackSessionManager.h"
#include "services/StreamActions.h"

#include <QSignalSpy>
#include <QString>
#include <QTest>
#include <QUuid>
#include <QVector>

#include <utility>

using namespace kinema;
using namespace kinema::playback::events;
using namespace kinema::playback::series;
using namespace kinema::playback::session;
namespace ports = kinema::playback::ports;

namespace {

// In-memory file catalog driven by the test.
class FakeCatalog final : public ports::SessionFileCatalog
{
public:
    QVector<domain::MediaFileEntry> filesForStreamRef(
        const domain::HistoryStreamRef& ref) const override
    {
        if (lookupRecord) {
            *lookupRecord = ref.infoHash;
        }
        return files;
    }
    QVector<domain::MediaFileEntry> filesForAssetId(
        const QString&) const override
    {
        return files;
    }

    QVector<domain::MediaFileEntry> files;
    QString* lookupRecord = nullptr;
};

// Subclass that records `play()` calls instead of dispatching.
class RecordingSessionManager final : public PlaybackSessionManager
{
public:
    using PlaybackSessionManager::PlaybackSessionManager;
    void play(const domain::Stream& stream,
        const domain::PlaybackContext& ctx) override
    {
        playCalls.append({ stream, ctx });
    }
    QList<QPair<domain::Stream, domain::PlaybackContext>> playCalls;
};

domain::MediaFileEntry makeFile(int index, const QString& path,
    qint64 size = 800LL * 1024 * 1024)
{
    domain::MediaFileEntry e;
    e.index = index;
    e.path = path;
    e.size = size;
    e.playable = true;
    return e;
}

QVector<domain::MediaFileEntry> packS01E01_E02_E03()
{
    return {
        makeFile(0, QStringLiteral("Show/Show.S01E01.mkv")),
        makeFile(1, QStringLiteral("Show/Show.S01E02.mkv")),
        makeFile(2, QStringLiteral("Show/Show.S01E03.mkv")),
    };
}

domain::PlaybackContext makeSeriesCtx(int season, int episode,
    const QString& infoHash
    = QStringLiteral("aabb1122ccdd3344eeff5566778899aabbccddee"))
{
    domain::PlaybackContext ctx;
    ctx.key.kind = domain::MediaKind::Series;
    ctx.key.imdbId = QStringLiteral("tt7654321");
    ctx.key.season = season;
    ctx.key.episode = episode;
    ctx.seriesTitle = QStringLiteral("Show");
    ctx.streamRef.infoHash = infoHash;
    return ctx;
}

domain::PlaybackContext makeMovieCtx()
{
    domain::PlaybackContext ctx;
    ctx.key.kind = domain::MediaKind::Movie;
    ctx.key.imdbId = QStringLiteral("tt1111111");
    ctx.streamRef.infoHash
        = QStringLiteral("0000000000000000000000000000000000000000");
    return ctx;
}

kinema::playback::PlaybackSessionId fresh()
{
    return QUuid::createUuid();
}

} // namespace

class TstSeriesSessionService : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void init()
    {
        m_actions = std::make_unique<services::StreamActions>(nullptr, nullptr);
        m_events = std::make_unique<PlaybackEventStream>();
        m_catalog = std::make_unique<FakeCatalog>();
        m_mgr = std::make_unique<RecordingSessionManager>(*m_actions,
            *m_events, nullptr, nullptr, nullptr);
        m_svc = std::make_unique<SeriesSessionService>(*m_events,
            *m_catalog, *m_mgr);
    }

    void cleanup()
    {
        m_svc.reset();
        m_mgr.reset();
        m_catalog.reset();
        m_events.reset();
        m_actions.reset();
    }

    // -----------------------------------------------------------------
    // Non-series context is ignored: navigation stays hidden and no
    // adjacency event fires when PlayerLoaded arrives.
    // -----------------------------------------------------------------
    void nonSeriesContextIsIgnored()
    {
        m_catalog->files = packS01E01_E02_E03();
        QSignalSpy navSpy(m_svc.get(),
            &SeriesSessionService::navigationChanged);
        QSignalSpy adjSpy(m_svc.get(),
            &SeriesSessionService::packAdjacencyResolved);

        const auto sid = fresh();
        m_events->publish(PlaybackRequested { sid, makeMovieCtx() });
        m_events->publish(PlayerLoaded { sid });

        QVERIFY(!m_svc->navigationVisible());
        QCOMPARE(adjSpy.size(), 0);
        // navigationChanged may fire on PlaybackRequested clear,
        // but it must not flip to visible.
        QVERIFY(!m_svc->canGoNext());
        QVERIFY(!m_svc->canGoPrevious());
    }

    // -----------------------------------------------------------------
    // Empty catalog clears navigation (no files yet -> no adjacency).
    // -----------------------------------------------------------------
    void emptyCatalogClearsNavigation()
    {
        m_catalog->files = {}; // no files
        const auto sid = fresh();
        m_events->publish(PlaybackRequested { sid, makeSeriesCtx(1, 2) });
        m_events->publish(PlayerLoaded { sid });

        QVERIFY(!m_svc->navigationVisible());
        QVERIFY(!m_svc->canGoNext());
        QVERIFY(!m_svc->canGoPrevious());
    }

    // -----------------------------------------------------------------
    // Ambiguous current (two files parse to the same S/E) -> clear.
    // -----------------------------------------------------------------
    void ambiguousCurrentClearsNavigation()
    {
        m_catalog->files = {
            makeFile(0, QStringLiteral("Show.S01E02.mkv")),
            makeFile(1, QStringLiteral("Show.S01E02.PROPER.mkv")),
        };
        const auto sid = fresh();
        m_events->publish(PlaybackRequested { sid, makeSeriesCtx(1, 2) });
        m_events->publish(PlayerLoaded { sid });

        QVERIFY(!m_svc->navigationVisible());
    }

    // -----------------------------------------------------------------
    // Next available -> navigationChanged + packAdjacencyResolved(true).
    // -----------------------------------------------------------------
    void nextAvailablePublishesAdjacencyAndShowsNavigation()
    {
        m_catalog->files = packS01E01_E02_E03();
        QSignalSpy navSpy(m_svc.get(),
            &SeriesSessionService::navigationChanged);
        QSignalSpy adjSpy(m_svc.get(),
            &SeriesSessionService::packAdjacencyResolved);

        const auto sid = fresh();
        m_events->publish(PlaybackRequested { sid, makeSeriesCtx(1, 2) });
        m_events->publish(PlayerLoaded { sid });

        QVERIFY(m_svc->navigationVisible());
        QVERIFY(m_svc->canGoPrevious());
        QVERIFY(m_svc->canGoNext());
        QVERIFY(adjSpy.size() >= 1);
        const auto last = adjSpy.last();
        QCOMPARE(last.at(0).toBool(), true);
        QCOMPARE(last.at(1).toInt(), 1);
        QCOMPARE(last.at(2).toInt(), 3);
        // navSpy fires at least once on the transition.
        QVERIFY(navSpy.size() >= 1);
    }

    // -----------------------------------------------------------------
    // packAdjacencyResolved de-dups: repeated PlayerLoaded for the
    // same (infoHash, S, E) does not re-fire.
    // -----------------------------------------------------------------
    void packAdjacencyDeDuplicatesPerEpisode()
    {
        m_catalog->files = packS01E01_E02_E03();
        QSignalSpy adjSpy(m_svc.get(),
            &SeriesSessionService::packAdjacencyResolved);

        const auto sid = fresh();
        m_events->publish(PlaybackRequested { sid, makeSeriesCtx(1, 2) });
        m_events->publish(PlayerLoaded { sid });
        m_events->publish(PlayerLoaded { sid });
        m_events->publish(PlayerLoaded { sid });

        QCOMPARE(adjSpy.size(), 1);
    }

    // -----------------------------------------------------------------
    // currentStreamSizeResolved fires once per (hash, fileIndex, size).
    // -----------------------------------------------------------------
    void currentStreamSizeIsHydratedOnce()
    {
        auto files = packS01E01_E02_E03();
        files[1].size = 1'234'567'890LL;
        m_catalog->files = files;

        QSignalSpy spy(m_svc.get(),
            &SeriesSessionService::currentStreamSizeResolved);

        const auto sid = fresh();
        m_events->publish(PlaybackRequested { sid, makeSeriesCtx(1, 2) });
        m_events->publish(PlayerLoaded { sid });
        m_events->publish(PlayerLoaded { sid });

        QCOMPARE(spy.size(), 1);
        QCOMPARE(spy.first().at(1).toInt(), 1); // fileIndex
        QCOMPARE(spy.first().at(2).toLongLong(), qint64(1'234'567'890LL));
    }

    // -----------------------------------------------------------------
    // Clean EOF with next available -> dispatches play() with the
    // next file's fields. No windowCloseRequested.
    // -----------------------------------------------------------------
    void naturalEofWithNextDispatchesPlay()
    {
        m_catalog->files = packS01E01_E02_E03();
        QSignalSpy closeSpy(m_svc.get(),
            &SeriesSessionService::windowCloseRequested);

        const auto sid = fresh();
        const auto ctx = makeSeriesCtx(1, 2);
        m_events->publish(PlaybackRequested { sid, ctx });
        m_events->publish(PlayerLoaded { sid });
        QVERIFY(m_svc->canGoNext());

        m_events->publish(PlaybackEnded {
            sid, kinema::playback::PlaybackEndReason::NaturalEof, ctx });

        QCOMPARE(m_mgr->playCalls.size(), 1);
        const auto& dispatched = m_mgr->playCalls.first();
        QCOMPARE(dispatched.first.infoHash, ctx.streamRef.infoHash);
        QCOMPARE(dispatched.first.fileIndex, 2);
        QCOMPARE(dispatched.second.key.season.value_or(0), 1);
        QCOMPARE(dispatched.second.key.episode.value_or(0), 3);
        QCOMPARE(closeSpy.size(), 0);
        // After dispatch the local adjacency state clears so the
        // chrome doesn't briefly show stale prev/next.
        QVERIFY(!m_svc->navigationVisible());
    }

    // -----------------------------------------------------------------
    // Clean EOF with no next -> windowCloseRequested, no play().
    // -----------------------------------------------------------------
    void naturalEofWithoutNextRequestsWindowClose()
    {
        m_catalog->files = packS01E01_E02_E03();
        QSignalSpy closeSpy(m_svc.get(),
            &SeriesSessionService::windowCloseRequested);

        const auto sid = fresh();
        const auto ctx = makeSeriesCtx(1, 3); // last episode
        m_events->publish(PlaybackRequested { sid, ctx });
        m_events->publish(PlayerLoaded { sid });
        QVERIFY(!m_svc->canGoNext());

        m_events->publish(PlaybackEnded {
            sid, kinema::playback::PlaybackEndReason::NaturalEof, ctx });

        QCOMPARE(m_mgr->playCalls.size(), 0);
        QCOMPARE(closeSpy.size(), 1);
    }

    // -----------------------------------------------------------------
    // UserStop never auto-nexts.
    // -----------------------------------------------------------------
    void userStopDoesNotAutoNext()
    {
        m_catalog->files = packS01E01_E02_E03();
        QSignalSpy closeSpy(m_svc.get(),
            &SeriesSessionService::windowCloseRequested);

        const auto sid = fresh();
        const auto ctx = makeSeriesCtx(1, 2);
        m_events->publish(PlaybackRequested { sid, ctx });
        m_events->publish(PlayerLoaded { sid });
        QVERIFY(m_svc->canGoNext());

        m_events->publish(PlaybackEnded {
            sid, kinema::playback::PlaybackEndReason::UserStop, ctx });

        QCOMPARE(m_mgr->playCalls.size(), 0);
        QCOMPARE(closeSpy.size(), 0);
        QVERIFY(!m_svc->navigationVisible());
    }

    // -----------------------------------------------------------------
    // PlaybackEnded from a superseded session is ignored.
    // -----------------------------------------------------------------
    void staleEndOfFileIsIgnored()
    {
        m_catalog->files = packS01E01_E02_E03();

        const auto first = fresh();
        const auto firstCtx = makeSeriesCtx(1, 2);
        m_events->publish(PlaybackRequested { first, firstCtx });
        m_events->publish(PlayerLoaded { first });

        // A new attempt supersedes.
        const auto second = fresh();
        m_events->publish(PlaybackRequested { second, makeSeriesCtx(1, 3) });

        // Stale EOF from the first session — must not auto-next.
        m_events->publish(PlaybackEnded {
            first, kinema::playback::PlaybackEndReason::NaturalEof,
            firstCtx });

        QCOMPARE(m_mgr->playCalls.size(), 0);
    }

    // -----------------------------------------------------------------
    // playNextEpisode / playPreviousEpisode dispatch through the
    // session manager.
    // -----------------------------------------------------------------
    void explicitNextAndPreviousDispatch()
    {
        m_catalog->files = packS01E01_E02_E03();
        const auto sid = fresh();
        m_events->publish(PlaybackRequested { sid, makeSeriesCtx(1, 2) });
        m_events->publish(PlayerLoaded { sid });

        m_svc->playNextEpisode();
        QCOMPARE(m_mgr->playCalls.size(), 1);
        QCOMPARE(m_mgr->playCalls.last().second.key.episode.value_or(0), 3);

        m_svc->playPreviousEpisode();
        QCOMPARE(m_mgr->playCalls.size(), 2);
        QCOMPARE(m_mgr->playCalls.last().second.key.episode.value_or(0), 1);
    }

    // -----------------------------------------------------------------
    // The catalog lookup uses the playback context's info hash; a
    // debrid catalog (same shape, different source) is handled
    // identically by the service.
    // -----------------------------------------------------------------
    void debridAndTorrentCatalogsBehaveIdentically()
    {
        m_catalog->files = packS01E01_E02_E03();
        QString seen;
        m_catalog->lookupRecord = &seen;

        const auto sid = fresh();
        const QString debridHash
            = QStringLiteral("ffffffffffffffffffffffffffffffffffffffff");
        m_events->publish(PlaybackRequested {
            sid, makeSeriesCtx(1, 2, debridHash) });
        m_events->publish(PlayerLoaded { sid });

        QCOMPARE(seen, debridHash);
        QVERIFY(m_svc->navigationVisible());
    }

private:
    std::unique_ptr<services::StreamActions> m_actions;
    std::unique_ptr<PlaybackEventStream> m_events;
    std::unique_ptr<FakeCatalog> m_catalog;
    std::unique_ptr<RecordingSessionManager> m_mgr;
    std::unique_ptr<SeriesSessionService> m_svc;
};

QTEST_MAIN(TstSeriesSessionService)
#include "tst_series_session_service.moc"
