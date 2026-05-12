// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "TestDoubles.h"

#include "domain/PlaybackContext.h"
#include "config/AppSettings.h"
#include "controllers/HistoryController.h"
#include "core/mpv/MpvChapterList.h"
#include "core/persistence/Database.h"
#include "core/persistence/HistoryStore.h"
#include "services/StreamActions.h"

#include <KConfig>
#include <KSharedConfig>

#include <QDateTime>
#include <QTemporaryDir>
#include <QTest>

using namespace kinema;
using kinema::tests::FakeIndexer;
using kinema::tests::IndexerHarness;
using kinema::tests::drainEvents;

namespace {

class RecordingStreamActions : public services::StreamActions
{
public:
    explicit RecordingStreamActions(QObject* parent = nullptr)
        : services::StreamActions(/*launcher=*/nullptr,
              /*torrentStreaming=*/nullptr, parent)
    {
    }

    struct Call {
        domain::Stream stream;
        domain::PlaybackContext ctx;
    };

    QList<Call> calls;

    void play(const domain::Stream& stream,
        const domain::PlaybackContext& ctx) override
    {
        calls.append({ stream, ctx });
    }
};

domain::HistoryEntry makeHistoryEntry(const QString& imdb,
    const QString& title, const QString& releaseSuffix)
{
    domain::HistoryEntry e;
    e.key.kind = domain::MediaKind::Movie;
    e.key.imdbId = imdb;
    e.title = title;
    e.poster = QUrl(QStringLiteral("https://example.com/") + imdb);
    e.lastWatchedAt = QDateTime::currentDateTimeUtc();
    e.lastStream.infoHash = QStringLiteral("hash-") + releaseSuffix;
    e.lastStream.releaseName = QStringLiteral("Release.") + releaseSuffix;
    e.lastStream.resolution = QStringLiteral("1080p");
    e.lastStream.qualityLabel = QStringLiteral("Torrentio 1080p");
    e.lastStream.provider = QStringLiteral("YTS");
    return e;
}

domain::Stream makeStream(const QString& releaseSuffix,
    const QString& directUrl)
{
    domain::Stream s;
    s.infoHash = QStringLiteral("hash-") + releaseSuffix;
    s.releaseName = QStringLiteral("Release.") + releaseSuffix;
    s.resolution = QStringLiteral("1080p");
    s.qualityLabel = QStringLiteral("Torrentio 1080p");
    s.provider = QStringLiteral("YTS");
    s.directUrl = QUrl(directUrl);
    return s;
}

} // namespace

class TstHistoryController : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void init()
    {
        m_tmp = std::make_unique<QTemporaryDir>();
        QVERIFY(m_tmp->isValid());

        m_db = std::make_unique<core::Database>(
            m_tmp->filePath(QStringLiteral("kinema.db")), nullptr);
        QVERIFY(m_db->open());
        m_historyStore = std::make_unique<core::HistoryStore>(*m_db);
        m_config = KSharedConfig::openConfig(
            m_tmp->filePath(QStringLiteral("kinemarc")),
            KConfig::SimpleConfig);
        m_settings = std::make_unique<config::AppSettings>(m_config);
        m_indexers = std::make_unique<IndexerHarness>();
        m_actions = std::make_unique<RecordingStreamActions>();
        m_historyCtrl = std::make_unique<controllers::HistoryController>(
            *m_historyStore, m_indexers->selector(), m_rdToken);
        m_historyCtrl->setStreamActions(m_actions.get());
    }

    void cleanup()
    {
        m_historyCtrl.reset();
        m_actions.reset();
        m_indexers.reset();
        m_settings.reset();
        m_config.reset();
        m_historyStore.reset();
        m_db.reset();
        m_tmp.reset();
    }

    // ---- Session lifecycle: onEndOfFile dispatching ----------------

    void testEndOfFileNaturalEofFinishesEvenAtLowProgress()
    {
        domain::PlaybackContext ctx;
        ctx.key.kind = domain::MediaKind::Movie;
        ctx.key.imdbId = QStringLiteral("tt9100001");
        ctx.title = QStringLiteral("Natural EOF");
        m_historyCtrl->onPlayStarting(ctx);
        m_historyCtrl->onFileLoaded();
        m_historyCtrl->onDurationChanged(5000);
        m_historyCtrl->onPositionChanged(2500);
        m_historyCtrl->onEndOfFile(QStringLiteral("eof"));

        const auto got = m_historyStore->find(ctx.key);
        QVERIFY(got.has_value());
        QVERIFY(got->finished);
    }

    void testEndOfFileErrorAt99PercentDoesNotForceFinish()
    {
        // Error path forwards to record(), which still applies the
        // 0.9 tick threshold. Position is 0.95 -> the tick rule does
        // fire. To prove "never *forces*" we use 0.88 which is below
        // both passive and stop thresholds.
        domain::PlaybackContext ctx;
        ctx.key.kind = domain::MediaKind::Movie;
        ctx.key.imdbId = QStringLiteral("tt9100002");
        ctx.title = QStringLiteral("Error 88%");
        m_historyCtrl->onPlayStarting(ctx);
        m_historyCtrl->onFileLoaded();
        m_historyCtrl->onDurationChanged(5000);
        m_historyCtrl->onPositionChanged(4400); // 88%
        m_historyCtrl->onEndOfFile(QStringLiteral("error"));

        const auto got = m_historyStore->find(ctx.key);
        QVERIFY(got.has_value());
        QVERIFY(!got->finished);
    }

    void testEndOfFileStopAt86PercentMovieFinishes()
    {
        domain::PlaybackContext ctx;
        ctx.key.kind = domain::MediaKind::Movie;
        ctx.key.imdbId = QStringLiteral("tt9100003");
        ctx.title = QStringLiteral("Stop 86%");
        m_historyCtrl->onPlayStarting(ctx);
        m_historyCtrl->onFileLoaded();
        m_historyCtrl->onDurationChanged(5000);
        m_historyCtrl->onPositionChanged(4300); // 86% -> above 0.85
        m_historyCtrl->onEndOfFile(QStringLiteral("stop"));

        const auto got = m_historyStore->find(ctx.key);
        QVERIFY(got.has_value());
        QVERIFY(got->finished);
    }

    void testEndOfFileStopAt87PercentEpisodeDoesNotFinish()
    {
        domain::PlaybackContext ctx;
        ctx.key.kind = domain::MediaKind::Series;
        ctx.key.imdbId = QStringLiteral("tt9100004");
        ctx.key.season = 1;
        ctx.key.episode = 1;
        ctx.title = QStringLiteral("Episode 87%");
        m_historyCtrl->onPlayStarting(ctx);
        m_historyCtrl->onFileLoaded();
        m_historyCtrl->onDurationChanged(2700);
        m_historyCtrl->onPositionChanged(2349); // 87% -> below 0.90
        m_historyCtrl->onEndOfFile(QStringLiteral("stop"));

        const auto got = m_historyStore->find(ctx.key);
        QVERIFY(got.has_value());
        QVERIFY(!got->finished);
    }

    void testChapterMarkerFlowsToStoreOnStop()
    {
        // 70% played, but file has a chapter titled "End Credits"
        // starting at 68% -> finished via chapter authority,
        // overriding the 0.85 stop threshold.
        domain::PlaybackContext ctx;
        ctx.key.kind = domain::MediaKind::Movie;
        ctx.key.imdbId = QStringLiteral("tt9100005");
        ctx.title = QStringLiteral("Chapter Credits");
        m_historyCtrl->onPlayStarting(ctx);
        m_historyCtrl->onFileLoaded();
        m_historyCtrl->onDurationChanged(5000);
        m_historyCtrl->onPositionChanged(3500); // 70%

        core::chapters::ChapterList chapters;
        core::chapters::Chapter intro;
        intro.time = 0.0;
        intro.title = QStringLiteral("Opening");
        chapters.append(intro);
        core::chapters::Chapter credits;
        credits.time = 3400.0; // 68%
        credits.title = QStringLiteral("End Credits");
        chapters.append(credits);
        m_historyCtrl->onChaptersChanged(chapters);

        m_historyCtrl->onEndOfFile(QStringLiteral("stop"));

        const auto got = m_historyStore->find(ctx.key);
        QVERIFY(got.has_value());
        QVERIFY(got->finished);
    }

    void testChapterListClearedBetweenSessions()
    {
        // Session 1 has a credits chapter; session 2 for a different
        // key must not inherit it.
        domain::PlaybackContext first;
        first.key.kind = domain::MediaKind::Movie;
        first.key.imdbId = QStringLiteral("tt9100006");
        first.title = QStringLiteral("First");
        m_historyCtrl->onPlayStarting(first);
        m_historyCtrl->onFileLoaded();
        m_historyCtrl->onDurationChanged(5000);
        m_historyCtrl->onPositionChanged(3500);

        core::chapters::ChapterList chapters;
        core::chapters::Chapter credits;
        credits.time = 3400.0;
        credits.title = QStringLiteral("Credits");
        chapters.append(credits);
        m_historyCtrl->onChaptersChanged(chapters);
        m_historyCtrl->onEndOfFile(QStringLiteral("stop"));

        // Second session: 70% played, no chapters provided. Must NOT
        // be auto-finished using session 1's chapter list.
        domain::PlaybackContext second;
        second.key.kind = domain::MediaKind::Movie;
        second.key.imdbId = QStringLiteral("tt9100007");
        second.title = QStringLiteral("Second");
        m_historyCtrl->onPlayStarting(second);
        m_historyCtrl->onFileLoaded();
        m_historyCtrl->onDurationChanged(5000);
        m_historyCtrl->onPositionChanged(3500); // 70% -> below 0.85
        m_historyCtrl->onEndOfFile(QStringLiteral("stop"));

        const auto got = m_historyStore->find(second.key);
        QVERIFY(got.has_value());
        QVERIFY(!got->finished);
    }

    void testQuitMappedToUserStop()
    {
        // mpv's "quit" path is treated identically to "stop".
        domain::PlaybackContext ctx;
        ctx.key.kind = domain::MediaKind::Movie;
        ctx.key.imdbId = QStringLiteral("tt9100008");
        ctx.title = QStringLiteral("Quit");
        m_historyCtrl->onPlayStarting(ctx);
        m_historyCtrl->onFileLoaded();
        m_historyCtrl->onDurationChanged(5000);
        m_historyCtrl->onPositionChanged(4300); // 86%
        m_historyCtrl->onEndOfFile(QStringLiteral("quit"));

        const auto got = m_historyStore->find(ctx.key);
        QVERIFY(got.has_value());
        QVERIFY(got->finished);
    }

    void resumeDispatchesDirectly()
    {
        QCOMPARE(m_actions->calls.size(), 0);

        FakeIndexer::ScriptedCall sc;
        sc.suspend = true;
        sc.streams.append(makeStream(QStringLiteral("NEW"),
            QStringLiteral("https://rd.example/new")));
        m_indexers->fake().scriptedCalls.append(sc);

        const auto entry = makeHistoryEntry(QStringLiteral("ttdup"),
            QStringLiteral("Movie"), QStringLiteral("NEW"));
        m_historyCtrl->resumeFromHistory(entry);
        drainEvents(4);

        QCOMPARE(m_indexers->fake().callCount, 1);
        QCOMPARE(m_actions->calls.size(), 1);
        QCOMPARE(m_actions->calls[0].ctx.key.imdbId,
            QStringLiteral("ttdup"));
        QCOMPARE(m_actions->calls[0].ctx.title,
            QStringLiteral("Movie"));
        QCOMPARE(m_actions->calls[0].stream.directUrl,
            QUrl(QStringLiteral("https://rd.example/new")));
    }

private:
    std::unique_ptr<QTemporaryDir> m_tmp;
    std::unique_ptr<core::Database> m_db;
    std::unique_ptr<core::HistoryStore> m_historyStore;
    KSharedConfig::Ptr m_config;
    std::unique_ptr<config::AppSettings> m_settings;
    std::unique_ptr<IndexerHarness> m_indexers;
    std::unique_ptr<RecordingStreamActions> m_actions;
    std::unique_ptr<controllers::HistoryController> m_historyCtrl;
    QString m_rdToken { QStringLiteral("rd-token-stub") };
};

QTEST_MAIN(TstHistoryController)
#include "tst_history_controller.moc"
