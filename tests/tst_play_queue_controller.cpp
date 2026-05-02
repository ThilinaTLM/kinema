// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "TestDoubles.h"

#include "api/Media.h"
#include "api/PlaybackContext.h"
#include "api/QueueItem.h"
#include "config/AppSettings.h"
#include "controllers/PlayQueueController.h"
#include "core/Database.h"
#include "core/HttpError.h"
#include "core/PlayQueueStore.h"
#include "services/StreamActions.h"

#include <KConfig>
#include <KSharedConfig>

#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>

using namespace kinema;
using kinema::tests::drainEvents;
using kinema::tests::FakeTorrentioClient;

namespace {

/// `services::StreamActions` overridden to record dispatch calls
/// instead of spawning a real player. The base class's `play()` is
/// virtual specifically so controller tests like this one can swap
/// it out without standing up a `PlayerLauncher`.
class RecordingStreamActions : public services::StreamActions
{
public:
    explicit RecordingStreamActions(QObject* parent = nullptr)
        : services::StreamActions(/*launcher=*/nullptr, parent)
    {
    }

    struct Call {
        api::Stream stream;
        api::PlaybackContext ctx;
    };

    QList<Call> calls;

    void play(const api::Stream& stream,
        const api::PlaybackContext& ctx) override
    {
        calls.append({stream, ctx});
    }
};

api::Stream makeStream(const QString& releaseSuffix,
    const QString& directUrl)
{
    api::Stream s;
    s.infoHash = QStringLiteral("hash-") + releaseSuffix;
    s.releaseName = QStringLiteral("Release.") + releaseSuffix;
    s.resolution = QStringLiteral("1080p");
    s.qualityLabel = QStringLiteral("Torrentio 1080p");
    s.sizeBytes = 1234567890;
    s.provider = QStringLiteral("YTS");
    s.directUrl = QUrl(directUrl);
    return s;
}

api::PlaybackContext makeMovieCtx(const QString& imdb,
    const QString& title)
{
    api::PlaybackContext ctx;
    ctx.key.kind = api::MediaKind::Movie;
    ctx.key.imdbId = imdb;
    ctx.title = title;
    ctx.poster = QUrl(QStringLiteral("https://example.com/p.jpg"));
    return ctx;
}

api::PlaybackContext makeEpisodeCtx(const QString& imdb,
    int season, int episode, const QString& title)
{
    api::PlaybackContext ctx;
    ctx.key.kind = api::MediaKind::Series;
    ctx.key.imdbId = imdb;
    ctx.key.season = season;
    ctx.key.episode = episode;
    ctx.title = title;
    ctx.seriesTitle = title;
    ctx.episodeTitle = QStringLiteral("S%1E%2")
                           .arg(season).arg(episode);
    return ctx;
}

} // namespace

class TestPlayQueueController : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void initTestCase()
    {
        m_dir = std::make_unique<QTemporaryDir>();
        QVERIFY(m_dir->isValid());
    }

    void init()
    {
        ++m_seq;
        m_dbPath = m_dir->path() + QStringLiteral("/queue-")
            + QString::number(m_seq) + QStringLiteral(".db");
        m_db = std::make_unique<core::Database>(m_dbPath, nullptr);
        QVERIFY(m_db->open());
        m_store = std::make_unique<core::PlayQueueStore>(*m_db);

        m_config = KSharedConfig::openConfig(
            m_dir->filePath(QStringLiteral("kinemarc-")
                + QString::number(m_seq)),
            KConfig::SimpleConfig);
        m_settings = std::make_unique<config::AppSettings>(m_config);
        m_torrentio = std::make_unique<FakeTorrentioClient>();
        m_actions = std::make_unique<RecordingStreamActions>();
        m_rdToken = QStringLiteral("rd-token-stub");

        m_ctrl = std::make_unique<controllers::PlayQueueController>(
            *m_store, *m_torrentio, *m_actions, *m_settings,
            m_rdToken);
    }

    void cleanup()
    {
        m_ctrl.reset();
        m_actions.reset();
        m_torrentio.reset();
        m_settings.reset();
        m_config.reset();
        m_store.reset();
        m_db.reset();
    }

    // -----------------------------------------------------------
    //  Construction / loading
    // -----------------------------------------------------------

    void emptyOnFreshConstruction()
    {
        QVERIFY(m_ctrl->isEmpty());
        QCOMPARE(m_ctrl->activeIndex(), -1);
    }

    void hydratesPersistedQueueAtStartup()
    {
        // Pre-populate via the store.
        api::QueueItem it;
        it.order = 0;
        it.key.kind = api::MediaKind::Movie;
        it.key.imdbId = QStringLiteral("tt0001");
        it.title = QStringLiteral("Pre-existing");
        it.streamRef.infoHash = QStringLiteral("hash-tt0001");
        it.streamRef.releaseName = QStringLiteral("R");
        QVERIFY(m_store->insert(it) > 0);

        // Reconstruct controller \u2014 should see the row.
        m_ctrl.reset();
        m_ctrl = std::make_unique<controllers::PlayQueueController>(
            *m_store, *m_torrentio, *m_actions, *m_settings,
            m_rdToken);

        QCOMPARE(m_ctrl->items().size(), 1);
        QCOMPARE(m_ctrl->items()[0].key.imdbId,
            QStringLiteral("tt0001"));
        QCOMPARE(m_ctrl->items()[0].status,
            api::QueueItem::Status::Pending);
        QCOMPARE(m_ctrl->activeIndex(), -1);
    }

    // -----------------------------------------------------------
    //  Play Now
    // -----------------------------------------------------------

    void playNowOnEmptyQueueInsertsAndDispatches()
    {
        QSignalSpy insertedSpy(m_ctrl.get(),
            &controllers::PlayQueueController::itemInserted);
        QSignalSpy activeSpy(m_ctrl.get(),
            &controllers::PlayQueueController::activeIndexChanged);

        const auto s = makeStream(QStringLiteral("A"),
            QStringLiteral("https://rd.example/A"));
        const auto ctx = makeMovieCtx(QStringLiteral("ttA"),
            QStringLiteral("Movie A"));
        m_ctrl->playNow(s, ctx);

        QCOMPARE(m_ctrl->items().size(), 1);
        QCOMPARE(m_ctrl->activeIndex(), 0);
        QCOMPARE(insertedSpy.count(), 1);
        QCOMPARE(insertedSpy[0][0].toInt(), 0);
        QCOMPARE(activeSpy.count(), 1);

        // Cached URL path \u2014 dispatches synchronously without a
        // Torrentio round-trip.
        QCOMPARE(m_torrentio->callCount, 0);
        QCOMPARE(m_actions->calls.size(), 1);
        QCOMPARE(m_actions->calls[0].stream.directUrl,
            s.directUrl);
        QCOMPARE(m_actions->calls[0].stream.infoHash, s.infoHash);
        QCOMPARE(m_actions->calls[0].ctx.key.imdbId,
            QStringLiteral("ttA"));
    }

    void playNowDisplacesPreviousActiveToSlotOne()
    {
        const auto sA = makeStream(QStringLiteral("A"),
            QStringLiteral("https://rd.example/A"));
        const auto ctxA = makeMovieCtx(QStringLiteral("ttA"),
            QStringLiteral("Movie A"));
        m_ctrl->playNow(sA, ctxA);
        QCOMPARE(m_ctrl->activeIndex(), 0);
        QCOMPARE(m_ctrl->items().size(), 1);

        const auto sB = makeStream(QStringLiteral("B"),
            QStringLiteral("https://rd.example/B"));
        const auto ctxB = makeMovieCtx(QStringLiteral("ttB"),
            QStringLiteral("Movie B"));
        m_ctrl->playNow(sB, ctxB);

        QCOMPARE(m_ctrl->items().size(), 2);
        QCOMPARE(m_ctrl->activeIndex(), 0);
        QCOMPARE(m_ctrl->items()[0].key.imdbId,
            QStringLiteral("ttB"));
        QCOMPARE(m_ctrl->items()[1].key.imdbId,
            QStringLiteral("ttA"));
        // The displaced row reverts to Pending.
        QCOMPARE(m_ctrl->items()[1].status,
            api::QueueItem::Status::Pending);
        // The new active row is Active.
        QCOMPARE(m_ctrl->items()[0].status,
            api::QueueItem::Status::Active);
        QCOMPARE(m_actions->calls.size(), 2);
    }

    void playNowRejectsStreamWithoutDirectUrl()
    {
        api::Stream s;
        s.infoHash = QStringLiteral("hash-x");
        s.releaseName = QStringLiteral("X");
        const auto ctx = makeMovieCtx(QStringLiteral("ttX"),
            QStringLiteral("X"));

        QSignalSpy statusSpy(m_ctrl.get(),
            &controllers::PlayQueueController::statusMessage);
        m_ctrl->playNow(s, ctx);

        QVERIFY(m_ctrl->isEmpty());
        QVERIFY(m_actions->calls.isEmpty());
        QCOMPARE(statusSpy.count(), 1);
    }

    void playNowDuplicateNonActiveMovesToFrontOnce()
    {
        m_ctrl->playNow(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"), QStringLiteral("A")));
        m_ctrl->enqueue(makeStream(QStringLiteral("B1"),
                            QStringLiteral("https://rd.example/B1")),
            makeMovieCtx(QStringLiteral("ttB"), QStringLiteral("B")));
        m_ctrl->enqueue(makeStream(QStringLiteral("C"),
                            QStringLiteral("https://rd.example/C")),
            makeMovieCtx(QStringLiteral("ttC"), QStringLiteral("C")));

        m_ctrl->playNow(makeStream(QStringLiteral("B2"),
                            QStringLiteral("https://rd.example/B2")),
            makeMovieCtx(QStringLiteral("ttB"), QStringLiteral("B")));

        QCOMPARE(m_ctrl->items().size(), 3);
        QCOMPARE(m_ctrl->activeIndex(), 0);
        QCOMPARE(m_ctrl->items()[0].key.imdbId, QStringLiteral("ttB"));
        QCOMPARE(m_ctrl->items()[0].streamRef.releaseName,
            QStringLiteral("Release.B2"));
        QCOMPARE(m_ctrl->items()[1].key.imdbId, QStringLiteral("ttA"));
        QCOMPARE(m_ctrl->items()[2].key.imdbId, QStringLiteral("ttC"));
        QCOMPARE(m_actions->calls.size(), 2);
    }

    void playNowDuplicateActiveRestartsWithoutInserting()
    {
        m_ctrl->playNow(makeStream(QStringLiteral("A1"),
                            QStringLiteral("https://rd.example/A1")),
            makeMovieCtx(QStringLiteral("ttA"), QStringLiteral("A")));

        m_ctrl->playNow(makeStream(QStringLiteral("A2"),
                            QStringLiteral("https://rd.example/A2")),
            makeMovieCtx(QStringLiteral("ttA"), QStringLiteral("A")));

        QCOMPARE(m_ctrl->items().size(), 1);
        QCOMPARE(m_ctrl->activeIndex(), 0);
        QCOMPARE(m_ctrl->items()[0].streamRef.releaseName,
            QStringLiteral("Release.A2"));
        QCOMPARE(m_actions->calls.size(), 2);
        QCOMPARE(m_actions->calls[1].stream.directUrl,
            QUrl(QStringLiteral("https://rd.example/A2")));
    }

    // -----------------------------------------------------------
    //  Play Next
    // -----------------------------------------------------------

    void playNextInsertsAfterActive()
    {
        m_ctrl->playNow(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"),
                QStringLiteral("A")));
        // queue: [A*]
        m_ctrl->enqueue(makeStream(QStringLiteral("Z"),
                            QStringLiteral("https://rd.example/Z")),
            makeMovieCtx(QStringLiteral("ttZ"),
                QStringLiteral("Z")));
        // queue: [A*, Z]
        QCOMPARE(m_ctrl->items().size(), 2);

        m_ctrl->playNext(makeStream(QStringLiteral("M"),
                             QStringLiteral("https://rd.example/M")),
            makeMovieCtx(QStringLiteral("ttM"),
                QStringLiteral("M")));
        // queue: [A*, M, Z]
        QCOMPARE(m_ctrl->items().size(), 3);
        QCOMPARE(m_ctrl->items()[0].key.imdbId, QStringLiteral("ttA"));
        QCOMPARE(m_ctrl->items()[1].key.imdbId, QStringLiteral("ttM"));
        QCOMPARE(m_ctrl->items()[2].key.imdbId, QStringLiteral("ttZ"));
        QCOMPARE(m_ctrl->activeIndex(), 0);
        // Did NOT preempt: only one dispatch \u2014 the original
        // playNow.
        QCOMPARE(m_actions->calls.size(), 1);
    }

    void playNextOnEmptyQueueInsertsAtZeroAndDoesNotPlay()
    {
        m_ctrl->playNext(makeStream(QStringLiteral("A"),
                             QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"),
                QStringLiteral("A")));
        QCOMPARE(m_ctrl->items().size(), 1);
        QCOMPARE(m_ctrl->activeIndex(), -1);
        QVERIFY(m_actions->calls.isEmpty());
    }

    void playNextDuplicateMovesBehindActiveOnce()
    {
        m_ctrl->playNow(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"), QStringLiteral("A")));
        m_ctrl->enqueue(makeStream(QStringLiteral("B1"),
                            QStringLiteral("https://rd.example/B1")),
            makeMovieCtx(QStringLiteral("ttB"), QStringLiteral("B")));
        m_ctrl->enqueue(makeStream(QStringLiteral("C"),
                            QStringLiteral("https://rd.example/C")),
            makeMovieCtx(QStringLiteral("ttC"), QStringLiteral("C")));

        m_ctrl->playNext(makeStream(QStringLiteral("C2"),
                             QStringLiteral("https://rd.example/C2")),
            makeMovieCtx(QStringLiteral("ttC"), QStringLiteral("C")));

        QCOMPARE(m_ctrl->items().size(), 3);
        QCOMPARE(m_ctrl->items()[0].key.imdbId, QStringLiteral("ttA"));
        QCOMPARE(m_ctrl->items()[1].key.imdbId, QStringLiteral("ttC"));
        QCOMPARE(m_ctrl->items()[1].streamRef.releaseName,
            QStringLiteral("Release.C2"));
        QCOMPARE(m_ctrl->items()[2].key.imdbId, QStringLiteral("ttB"));
        QCOMPARE(m_actions->calls.size(), 1);
    }

    void playNextDuplicateActiveNoOpsWithStatus()
    {
        m_ctrl->playNow(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"), QStringLiteral("A")));

        QSignalSpy statusSpy(m_ctrl.get(),
            &controllers::PlayQueueController::statusMessage);
        m_ctrl->playNext(makeStream(QStringLiteral("A2"),
                             QStringLiteral("https://rd.example/A2")),
            makeMovieCtx(QStringLiteral("ttA"), QStringLiteral("A")));

        QCOMPARE(m_ctrl->items().size(), 1);
        QCOMPARE(m_actions->calls.size(), 1);
        QCOMPARE(statusSpy.count(), 1);
    }

    // -----------------------------------------------------------
    //  Add to queue
    // -----------------------------------------------------------

    void enqueueAppendsAndDoesNotPreempt()
    {
        m_ctrl->playNow(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"),
                QStringLiteral("A")));
        m_ctrl->enqueue(makeStream(QStringLiteral("B"),
                            QStringLiteral("https://rd.example/B")),
            makeMovieCtx(QStringLiteral("ttB"),
                QStringLiteral("B")));
        m_ctrl->enqueue(makeStream(QStringLiteral("C"),
                            QStringLiteral("https://rd.example/C")),
            makeMovieCtx(QStringLiteral("ttC"),
                QStringLiteral("C")));
        QCOMPARE(m_ctrl->items().size(), 3);
        QCOMPARE(m_ctrl->items()[0].key.imdbId,
            QStringLiteral("ttA"));
        QCOMPARE(m_ctrl->items()[2].key.imdbId,
            QStringLiteral("ttC"));
        QCOMPARE(m_ctrl->activeIndex(), 0);
        QCOMPARE(m_actions->calls.size(), 1);
    }

    void enqueueDuplicateAppendsOnce()
    {
        m_ctrl->playNow(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"), QStringLiteral("A")));
        m_ctrl->enqueue(makeStream(QStringLiteral("B1"),
                            QStringLiteral("https://rd.example/B1")),
            makeMovieCtx(QStringLiteral("ttB"), QStringLiteral("B")));
        m_ctrl->enqueue(makeStream(QStringLiteral("C"),
                            QStringLiteral("https://rd.example/C")),
            makeMovieCtx(QStringLiteral("ttC"), QStringLiteral("C")));

        m_ctrl->enqueue(makeStream(QStringLiteral("B2"),
                            QStringLiteral("https://rd.example/B2")),
            makeMovieCtx(QStringLiteral("ttB"), QStringLiteral("B")));

        QCOMPARE(m_ctrl->items().size(), 3);
        QCOMPARE(m_ctrl->items()[0].key.imdbId, QStringLiteral("ttA"));
        QCOMPARE(m_ctrl->items()[1].key.imdbId, QStringLiteral("ttC"));
        QCOMPARE(m_ctrl->items()[2].key.imdbId, QStringLiteral("ttB"));
        QCOMPARE(m_ctrl->items()[2].streamRef.releaseName,
            QStringLiteral("Release.B2"));
        QCOMPARE(m_actions->calls.size(), 1);
    }

    void enqueueDuplicateActiveNoOpsWithStatus()
    {
        m_ctrl->playNow(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"), QStringLiteral("A")));

        QSignalSpy statusSpy(m_ctrl.get(),
            &controllers::PlayQueueController::statusMessage);
        m_ctrl->enqueue(makeStream(QStringLiteral("A2"),
                            QStringLiteral("https://rd.example/A2")),
            makeMovieCtx(QStringLiteral("ttA"), QStringLiteral("A")));

        QCOMPARE(m_ctrl->items().size(), 1);
        QCOMPARE(m_actions->calls.size(), 1);
        QCOMPARE(statusSpy.count(), 1);
    }

    // -----------------------------------------------------------
    //  Persistence
    // -----------------------------------------------------------

    void enqueuePersistsBetweenInstances()
    {
        m_ctrl->enqueue(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"),
                QStringLiteral("A")));
        m_ctrl->enqueue(makeStream(QStringLiteral("B"),
                            QStringLiteral("https://rd.example/B")),
            makeMovieCtx(QStringLiteral("ttB"),
                QStringLiteral("B")));

        // Tear down + reload on the same database.
        m_ctrl.reset();
        m_ctrl = std::make_unique<controllers::PlayQueueController>(
            *m_store, *m_torrentio, *m_actions, *m_settings,
            m_rdToken);

        QCOMPARE(m_ctrl->items().size(), 2);
        QCOMPARE(m_ctrl->items()[0].key.imdbId,
            QStringLiteral("ttA"));
        QCOMPARE(m_ctrl->items()[1].key.imdbId,
            QStringLiteral("ttB"));
        QCOMPARE(m_ctrl->activeIndex(), -1);
        // Cached URLs are NOT persisted.
        QVERIFY(m_ctrl->items()[0].cachedDirectUrl.isEmpty());
    }

    // -----------------------------------------------------------
    //  removeAt
    // -----------------------------------------------------------

    void removeNonActiveDoesNotChangeActive()
    {
        m_ctrl->playNow(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"),
                QStringLiteral("A")));
        m_ctrl->enqueue(makeStream(QStringLiteral("B"),
                            QStringLiteral("https://rd.example/B")),
            makeMovieCtx(QStringLiteral("ttB"),
                QStringLiteral("B")));
        QCOMPARE(m_ctrl->activeIndex(), 0);

        m_ctrl->removeAt(1);
        QCOMPARE(m_ctrl->items().size(), 1);
        QCOMPARE(m_ctrl->activeIndex(), 0);
    }

    void removeActiveAdvancesToNext()
    {
        m_ctrl->playNow(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"),
                QStringLiteral("A")));
        m_ctrl->enqueue(makeStream(QStringLiteral("B"),
                            QStringLiteral("https://rd.example/B")),
            makeMovieCtx(QStringLiteral("ttB"),
                QStringLiteral("B")));
        QCOMPARE(m_actions->calls.size(), 1);

        m_ctrl->removeAt(0);
        // Step 4: removing the active row advances to the new
        // slot 0 (B) and dispatches it.
        QCOMPARE(m_ctrl->items().size(), 1);
        QCOMPARE(m_ctrl->activeIndex(), 0);
        QCOMPARE(m_ctrl->items()[0].key.imdbId,
            QStringLiteral("ttB"));
        QCOMPARE(m_actions->calls.size(), 2);
    }

    void removeOnlyActiveEmitsWindowCloseRequested()
    {
        m_ctrl->playNow(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"),
                QStringLiteral("A")));
        QSignalSpy closeSpy(m_ctrl.get(),
            &controllers::PlayQueueController::windowCloseRequested);
        m_ctrl->removeAt(0);
        QCOMPARE(m_ctrl->activeIndex(), -1);
        QVERIFY(m_ctrl->isEmpty());
        QCOMPARE(closeSpy.count(), 1);
    }

    void removeBeforeActiveShiftsActiveDown()
    {
        m_ctrl->enqueue(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"),
                QStringLiteral("A")));
        m_ctrl->enqueue(makeStream(QStringLiteral("B"),
                            QStringLiteral("https://rd.example/B")),
            makeMovieCtx(QStringLiteral("ttB"),
                QStringLiteral("B")));
        m_ctrl->playAt(1); // active = 1 (B)

        // Drain the resolve coroutine (cachedDirectUrl path is sync,
        // but defensive).
        drainEvents(2);

        QCOMPARE(m_ctrl->activeIndex(), 1);
        m_ctrl->removeAt(0); // remove A
        QCOMPARE(m_ctrl->items().size(), 1);
        QCOMPARE(m_ctrl->activeIndex(), 0); // B shifted down
    }

    // -----------------------------------------------------------
    //  moveTo
    // -----------------------------------------------------------

    void moveToReorders()
    {
        for (auto c : {QStringLiteral("A"), QStringLiteral("B"),
                 QStringLiteral("C")}) {
            m_ctrl->enqueue(makeStream(c,
                                QStringLiteral("https://rd.example/") + c),
                makeMovieCtx(QStringLiteral("tt") + c, c));
        }
        QCOMPARE(m_ctrl->items().size(), 3);

        m_ctrl->moveTo(2, 0); // C \u2192 front
        QCOMPARE(m_ctrl->items()[0].key.imdbId,
            QStringLiteral("ttC"));
        QCOMPARE(m_ctrl->items()[1].key.imdbId,
            QStringLiteral("ttA"));
        QCOMPARE(m_ctrl->items()[2].key.imdbId,
            QStringLiteral("ttB"));
    }

    void moveToTracksActiveIndex()
    {
        m_ctrl->playNow(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"),
                QStringLiteral("A")));
        m_ctrl->enqueue(makeStream(QStringLiteral("B"),
                            QStringLiteral("https://rd.example/B")),
            makeMovieCtx(QStringLiteral("ttB"),
                QStringLiteral("B")));
        m_ctrl->enqueue(makeStream(QStringLiteral("C"),
                            QStringLiteral("https://rd.example/C")),
            makeMovieCtx(QStringLiteral("ttC"),
                QStringLiteral("C")));
        QCOMPARE(m_ctrl->activeIndex(), 0);

        // Move active to slot 2.
        m_ctrl->moveTo(0, 2);
        QCOMPARE(m_ctrl->items()[2].key.imdbId,
            QStringLiteral("ttA"));
        QCOMPARE(m_ctrl->activeIndex(), 2);

        // Move a non-active row that crosses the active.
        m_ctrl->moveTo(0, 2); // B from 0 \u2192 2; active was 2 (A)
        // Result: [C, A, B]; active stays on A which is now at 1.
        QCOMPARE(m_ctrl->items()[0].key.imdbId,
            QStringLiteral("ttC"));
        QCOMPARE(m_ctrl->items()[1].key.imdbId,
            QStringLiteral("ttA"));
        QCOMPARE(m_ctrl->items()[2].key.imdbId,
            QStringLiteral("ttB"));
        QCOMPARE(m_ctrl->activeIndex(), 1);
    }

    // -----------------------------------------------------------
    //  Resolution failure path
    // -----------------------------------------------------------

    void torrentioFailureMarksFailedAndDoesNotDispatch()
    {
        // Persisted item with no cached URL — controller must
        // re-resolve via Torrentio.
        api::QueueItem it;
        it.order = 0;
        it.key.kind = api::MediaKind::Movie;
        it.key.imdbId = QStringLiteral("ttX");
        it.title = QStringLiteral("X");
        it.streamRef.infoHash = QStringLiteral("hash-X");
        it.streamRef.releaseName = QStringLiteral("Release.X");
        QVERIFY(m_store->insert(it) > 0);

        m_ctrl.reset();
        m_ctrl = std::make_unique<controllers::PlayQueueController>(
            *m_store, *m_torrentio, *m_actions, *m_settings,
            m_rdToken);
        QCOMPARE(m_ctrl->items().size(), 1);

        // Script Torrentio to throw on the next call.
        FakeTorrentioClient::ScriptedCall sc;
        sc.suspend = true; // co_await before throwing
        sc.error = core::HttpError{
            core::HttpError::Kind::HttpStatus, 503,
            QStringLiteral("Service Unavailable")};
        m_torrentio->scriptedCalls.append(sc);

        QSignalSpy changedSpy(m_ctrl.get(),
            &controllers::PlayQueueController::itemChanged);

        m_ctrl->playAt(0);
        // Drain the suspended coroutine so it resumes + throws.
        drainEvents(4);

        QCOMPARE(m_ctrl->items()[0].status,
            api::QueueItem::Status::Failed);
        QVERIFY(m_actions->calls.isEmpty());
        // First itemChanged sets Active, second sets Failed.
        QVERIFY(changedSpy.count() >= 2);
    }

    void torrentioInfoHashMissMarksFailed()
    {
        api::QueueItem it;
        it.order = 0;
        it.key.kind = api::MediaKind::Movie;
        it.key.imdbId = QStringLiteral("ttY");
        it.title = QStringLiteral("Y");
        it.streamRef.infoHash = QStringLiteral("hash-Y");
        it.streamRef.releaseName = QStringLiteral("Release.Y");
        QVERIFY(m_store->insert(it) > 0);

        m_ctrl.reset();
        m_ctrl = std::make_unique<controllers::PlayQueueController>(
            *m_store, *m_torrentio, *m_actions, *m_settings,
            m_rdToken);

        // Torrentio responds with streams but none match infoHash.
        FakeTorrentioClient::ScriptedCall sc;
        sc.suspend = true;
        sc.streams.append(makeStream(QStringLiteral("OTHER"),
            QStringLiteral("https://rd.example/other")));
        m_torrentio->scriptedCalls.append(sc);

        m_ctrl->playAt(0);
        drainEvents(4);

        QCOMPARE(m_ctrl->items()[0].status,
            api::QueueItem::Status::Failed);
        QVERIFY(m_actions->calls.isEmpty());
    }

    void torrentioInfoHashHitDispatches()
    {
        api::QueueItem it;
        it.order = 0;
        it.key.kind = api::MediaKind::Movie;
        it.key.imdbId = QStringLiteral("ttZ");
        it.title = QStringLiteral("Z");
        it.streamRef.infoHash = QStringLiteral("hash-Z");
        it.streamRef.releaseName = QStringLiteral("Release.Z");
        QVERIFY(m_store->insert(it) > 0);

        m_ctrl.reset();
        m_ctrl = std::make_unique<controllers::PlayQueueController>(
            *m_store, *m_torrentio, *m_actions, *m_settings,
            m_rdToken);

        FakeTorrentioClient::ScriptedCall sc;
        sc.suspend = true;
        sc.streams.append(makeStream(QStringLiteral("Z"),
            QStringLiteral("https://rd.example/Z-fresh")));
        m_torrentio->scriptedCalls.append(sc);

        m_ctrl->playAt(0);
        drainEvents(4);

        QCOMPARE(m_actions->calls.size(), 1);
        QCOMPARE(m_actions->calls[0].stream.directUrl,
            QUrl(QStringLiteral("https://rd.example/Z-fresh")));
        QCOMPARE(m_ctrl->items()[0].status,
            api::QueueItem::Status::Active);
    }

    // -----------------------------------------------------------
    //  Episode round-trip
    // -----------------------------------------------------------

    void episodeQueueRoundTrip()
    {
        const auto s = makeStream(QStringLiteral("E1"),
            QStringLiteral("https://rd.example/E1"));
        const auto ctx = makeEpisodeCtx(QStringLiteral("ttSer"),
            2, 5, QStringLiteral("Show"));
        m_ctrl->enqueue(s, ctx);

        m_ctrl.reset();
        m_ctrl = std::make_unique<controllers::PlayQueueController>(
            *m_store, *m_torrentio, *m_actions, *m_settings,
            m_rdToken);
        QCOMPARE(m_ctrl->items().size(), 1);
        const auto& q = m_ctrl->items()[0];
        QCOMPARE(q.key.kind, api::MediaKind::Series);
        QVERIFY(q.key.season.has_value());
        QCOMPARE(*q.key.season, 2);
        QVERIFY(q.key.episode.has_value());
        QCOMPARE(*q.key.episode, 5);
    }

    // -----------------------------------------------------------
    //  Step 4: end-of-file auto-advance
    // -----------------------------------------------------------

    void cleanEndOfFileMarksPlayedAndAdvances()
    {
        m_ctrl->playNow(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"),
                QStringLiteral("A")));
        m_ctrl->enqueue(makeStream(QStringLiteral("B"),
                            QStringLiteral("https://rd.example/B")),
            makeMovieCtx(QStringLiteral("ttB"),
                QStringLiteral("B")));
        QCOMPARE(m_actions->calls.size(), 1);

        m_ctrl->onPlayerEndOfFile(QStringLiteral("eof"),
            api::PlaybackContext {});

        // A is kept in place as `Played` so the user can replay it
        // via Previous; B becomes the new active row at slot 1.
        QCOMPARE(m_ctrl->items().size(), 2);
        QCOMPARE(m_ctrl->items()[0].key.imdbId,
            QStringLiteral("ttA"));
        QCOMPARE(m_ctrl->items()[0].status,
            api::QueueItem::Status::Played);
        QCOMPARE(m_ctrl->items()[1].key.imdbId,
            QStringLiteral("ttB"));
        QCOMPARE(m_ctrl->activeIndex(), 1);
        QCOMPARE(m_actions->calls.size(), 2);
    }

    void cleanEndOfFileOnLastItemMarksPlayedAndCloses()
    {
        m_ctrl->playNow(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"),
                QStringLiteral("A")));

        QSignalSpy closeSpy(m_ctrl.get(),
            &controllers::PlayQueueController::windowCloseRequested);
        m_ctrl->onPlayerEndOfFile(QStringLiteral("eof"),
            api::PlaybackContext {});

        // The played row stays so it remains discoverable on the
        // queue page; activeIndex drops to -1 and the player is
        // asked to close.
        QCOMPARE(m_ctrl->items().size(), 1);
        QCOMPARE(m_ctrl->items()[0].status,
            api::QueueItem::Status::Played);
        QCOMPARE(m_ctrl->activeIndex(), -1);
        QCOMPARE(closeSpy.count(), 1);
    }

    void errorEndOfFileMarksFailedAndAdvances()
    {
        m_ctrl->playNow(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"),
                QStringLiteral("A")));
        m_ctrl->enqueue(makeStream(QStringLiteral("B"),
                            QStringLiteral("https://rd.example/B")),
            makeMovieCtx(QStringLiteral("ttB"),
                QStringLiteral("B")));
        QCOMPARE(m_actions->calls.size(), 1);

        m_ctrl->onPlayerEndOfFile(QStringLiteral("error"),
            api::PlaybackContext {});

        // A stays in the queue but is marked Failed; B is now active.
        QCOMPARE(m_ctrl->items().size(), 2);
        QCOMPARE(m_ctrl->items()[0].status,
            api::QueueItem::Status::Failed);
        QCOMPARE(m_ctrl->activeIndex(), 1);
        QCOMPARE(m_actions->calls.size(), 2);
    }

    void userClosedRevertsActiveAndConsumesNextEof()
    {
        m_ctrl->playNow(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"),
                QStringLiteral("A")));
        m_ctrl->enqueue(makeStream(QStringLiteral("B"),
                            QStringLiteral("https://rd.example/B")),
            makeMovieCtx(QStringLiteral("ttB"),
                QStringLiteral("B")));
        QCOMPARE(m_actions->calls.size(), 1);

        m_ctrl->onPlayerUserClosed();
        // The mpv stop after closeEvent emits an EOF with reason
        // "stop" - the queue must NOT advance.
        m_ctrl->onPlayerEndOfFile(QStringLiteral("stop"),
            api::PlaybackContext {});

        // Queue intact, active row reverted to Pending.
        QCOMPARE(m_ctrl->items().size(), 2);
        QCOMPARE(m_ctrl->activeIndex(), 0);
        QCOMPARE(m_ctrl->items()[0].status,
            api::QueueItem::Status::Pending);
        // No new dispatch.
        QCOMPARE(m_actions->calls.size(), 1);
    }

    void previousAndNextItemNavigateWithinBounds()
    {
        m_ctrl->playNow(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"), QStringLiteral("A")));
        m_ctrl->enqueue(makeStream(QStringLiteral("B"),
                            QStringLiteral("https://rd.example/B")),
            makeMovieCtx(QStringLiteral("ttB"), QStringLiteral("B")));
        m_ctrl->enqueue(makeStream(QStringLiteral("C"),
                            QStringLiteral("https://rd.example/C")),
            makeMovieCtx(QStringLiteral("ttC"), QStringLiteral("C")));

        FakeTorrentioClient::ScriptedCall sc;
        sc.suspend = true;
        sc.streams.append(makeStream(QStringLiteral("A"),
            QStringLiteral("https://rd.example/A-fresh")));
        m_torrentio->scriptedCalls.append(sc);

        m_ctrl->playNextItem();
        QCOMPARE(m_ctrl->activeIndex(), 1);
        QCOMPARE(m_actions->calls.size(), 2);

        m_ctrl->playPreviousItem();
        drainEvents(4);
        QCOMPARE(m_ctrl->activeIndex(), 0);
        QCOMPARE(m_actions->calls.size(), 3);

        m_ctrl->playPreviousItem();
        QCOMPARE(m_ctrl->activeIndex(), 0);
        QCOMPARE(m_actions->calls.size(), 3);
    }

    void freshDispatchClearsPendingUserClose()
    {
        m_ctrl->playNow(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"), QStringLiteral("A")));
        m_ctrl->enqueue(makeStream(QStringLiteral("B"),
                            QStringLiteral("https://rd.example/B")),
            makeMovieCtx(QStringLiteral("ttB"), QStringLiteral("B")));

        FakeTorrentioClient::ScriptedCall sc;
        sc.suspend = true;
        sc.streams.append(makeStream(QStringLiteral("A"),
            QStringLiteral("https://rd.example/A-fresh")));
        m_torrentio->scriptedCalls.append(sc);

        m_ctrl->onPlayerUserClosed();
        m_ctrl->playAt(0);
        drainEvents(4);
        m_ctrl->onPlayerEndOfFile(QStringLiteral("eof"),
            api::PlaybackContext {});

        // The clean EOF marks A as Played in place and advances to B.
        QCOMPARE(m_ctrl->items().size(), 2);
        QCOMPARE(m_ctrl->items()[0].key.imdbId, QStringLiteral("ttA"));
        QCOMPARE(m_ctrl->items()[0].status,
            api::QueueItem::Status::Played);
        QCOMPARE(m_ctrl->items()[1].key.imdbId, QStringLiteral("ttB"));
        QCOMPARE(m_ctrl->activeIndex(), 1);
        QCOMPARE(m_actions->calls.size(), 3);
    }

    // -----------------------------------------------------------
    //  clearAll / clearAllExceptActive
    // -----------------------------------------------------------

    void clearAllExceptActiveKeepsOnlyCurrentItem()
    {
        m_ctrl->playNow(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"), QStringLiteral("A")));
        m_ctrl->enqueue(makeStream(QStringLiteral("B"),
                            QStringLiteral("https://rd.example/B")),
            makeMovieCtx(QStringLiteral("ttB"), QStringLiteral("B")));
        m_ctrl->enqueue(makeStream(QStringLiteral("C"),
                            QStringLiteral("https://rd.example/C")),
            makeMovieCtx(QStringLiteral("ttC"), QStringLiteral("C")));

        QSignalSpy resetSpy(m_ctrl.get(),
            &controllers::PlayQueueController::itemsReset);

        m_ctrl->clearAllExceptActive();

        QCOMPARE(resetSpy.count(), 1);
        QCOMPARE(m_ctrl->items().size(), 1);
        QCOMPARE(m_ctrl->activeIndex(), 0);
        QCOMPARE(m_ctrl->items()[0].key.imdbId, QStringLiteral("ttA"));

        const auto persisted = m_store->loadAll();
        QCOMPARE(persisted.size(), 1);
        QCOMPARE(persisted[0].key.imdbId, QStringLiteral("ttA"));
        QCOMPARE(persisted[0].order, 0);
    }

    void clearAllExceptActiveWithoutActiveFallsBackToClearAll()
    {
        m_ctrl->enqueue(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"),
                QStringLiteral("A")));
        m_ctrl->enqueue(makeStream(QStringLiteral("B"),
                            QStringLiteral("https://rd.example/B")),
            makeMovieCtx(QStringLiteral("ttB"),
                QStringLiteral("B")));
        QCOMPARE(m_ctrl->activeIndex(), -1);

        m_ctrl->clearAllExceptActive();
        QVERIFY(m_ctrl->isEmpty());
        QCOMPARE(m_ctrl->activeIndex(), -1);
        QVERIFY(m_store->loadAll().isEmpty());
    }

    void clearAllEmptiesQueueAndPersists()
    {
        m_ctrl->enqueue(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"),
                QStringLiteral("A")));
        m_ctrl->enqueue(makeStream(QStringLiteral("B"),
                            QStringLiteral("https://rd.example/B")),
            makeMovieCtx(QStringLiteral("ttB"),
                QStringLiteral("B")));

        m_ctrl->clearAll();
        QVERIFY(m_ctrl->isEmpty());
        QCOMPARE(m_ctrl->activeIndex(), -1);
        // Persisted clear.
        QVERIFY(m_store->loadAll().isEmpty());
    }

    // -----------------------------------------------------------
    //  Played-status: clearPlayed + persistence + cap
    // -----------------------------------------------------------

    void clearPlayedDropsPlayedRowsOnly()
    {
        // Build [A(active), B, C] then EOF A so it becomes Played
        // and B becomes active. State: [A(played), B(active), C].
        m_ctrl->playNow(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"), QStringLiteral("A")));
        m_ctrl->enqueue(makeStream(QStringLiteral("B"),
                            QStringLiteral("https://rd.example/B")),
            makeMovieCtx(QStringLiteral("ttB"), QStringLiteral("B")));
        m_ctrl->enqueue(makeStream(QStringLiteral("C"),
                            QStringLiteral("https://rd.example/C")),
            makeMovieCtx(QStringLiteral("ttC"), QStringLiteral("C")));
        m_ctrl->onPlayerEndOfFile(QStringLiteral("eof"),
            api::PlaybackContext {});
        QCOMPARE(m_ctrl->items().size(), 3);
        QCOMPARE(m_ctrl->items()[0].status,
            api::QueueItem::Status::Played);
        QCOMPARE(m_ctrl->activeIndex(), 1);

        m_ctrl->clearPlayed();

        QCOMPARE(m_ctrl->items().size(), 2);
        QCOMPARE(m_ctrl->items()[0].key.imdbId, QStringLiteral("ttB"));
        QCOMPARE(m_ctrl->items()[1].key.imdbId, QStringLiteral("ttC"));
        QCOMPARE(m_ctrl->activeIndex(), 0);
    }

    void playedRowsAreNotPersistedAcrossLoad()
    {
        // [A(active), B] → EOF A → [A(played), B(active)].
        m_ctrl->playNow(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"), QStringLiteral("A")));
        m_ctrl->enqueue(makeStream(QStringLiteral("B"),
                            QStringLiteral("https://rd.example/B")),
            makeMovieCtx(QStringLiteral("ttB"), QStringLiteral("B")));
        m_ctrl->onPlayerEndOfFile(QStringLiteral("eof"),
            api::PlaybackContext {});

        // The store snapshot must contain only the non-played row.
        const auto persisted = m_store->loadAll();
        QCOMPARE(persisted.size(), 1);
        QCOMPARE(persisted[0].key.imdbId, QStringLiteral("ttB"));
    }

    void playNowInsertsAtActiveSlotAboveDemotedActive()
    {
        // Start with [A(played), B(active)] by playing A and EOFing
        // it after enqueueing B.
        m_ctrl->playNow(makeStream(QStringLiteral("A"),
                            QStringLiteral("https://rd.example/A")),
            makeMovieCtx(QStringLiteral("ttA"), QStringLiteral("A")));
        m_ctrl->enqueue(makeStream(QStringLiteral("B"),
                            QStringLiteral("https://rd.example/B")),
            makeMovieCtx(QStringLiteral("ttB"), QStringLiteral("B")));
        m_ctrl->onPlayerEndOfFile(QStringLiteral("eof"),
            api::PlaybackContext {});
        QCOMPARE(m_ctrl->activeIndex(), 1);

        // playNow on a brand-new release C should insert at the
        // current active slot (1), demoting B to slot 2 as Pending,
        // and leave A's `Played` status above untouched.
        m_ctrl->playNow(makeStream(QStringLiteral("C"),
                            QStringLiteral("https://rd.example/C")),
            makeMovieCtx(QStringLiteral("ttC"), QStringLiteral("C")));

        QCOMPARE(m_ctrl->items().size(), 3);
        QCOMPARE(m_ctrl->items()[0].key.imdbId, QStringLiteral("ttA"));
        QCOMPARE(m_ctrl->items()[0].status,
            api::QueueItem::Status::Played);
        QCOMPARE(m_ctrl->items()[1].key.imdbId, QStringLiteral("ttC"));
        QCOMPARE(m_ctrl->items()[2].key.imdbId, QStringLiteral("ttB"));
        QCOMPARE(m_ctrl->items()[2].status,
            api::QueueItem::Status::Pending);
        QCOMPARE(m_ctrl->activeIndex(), 1);
    }

private:
    std::unique_ptr<QTemporaryDir> m_dir;
    QString m_dbPath;
    int m_seq = 0;
    std::unique_ptr<core::Database> m_db;
    std::unique_ptr<core::PlayQueueStore> m_store;
    KSharedConfig::Ptr m_config;
    std::unique_ptr<config::AppSettings> m_settings;
    std::unique_ptr<FakeTorrentioClient> m_torrentio;
    std::unique_ptr<RecordingStreamActions> m_actions;
    QString m_rdToken;
    std::unique_ptr<controllers::PlayQueueController> m_ctrl;
};

QTEST_MAIN(TestPlayQueueController)
#include "tst_play_queue_controller.moc"
