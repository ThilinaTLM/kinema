// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/OpenSubtitlesClient.h"
#include "api/Subtitle.h"
#include "config/AppSettings.h"
#include "config/CacheSettings.h"
#include "config/SubtitleSettings.h"
#include "controllers/SubtitleController.h"
#include "core/CachePaths.h"
#include "core/Database.h"
#include "core/HttpError.h"
#include "core/SubtitleCacheStore.h"

#include <KSharedConfig>

#include <QDir>
#include <QFile>
#include <QSignalSpy>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTest>

#include <QCoro/QCoroTask>

using namespace kinema;

namespace {

class FakeOpenSubtitles : public api::OpenSubtitlesClient
{
public:
    FakeOpenSubtitles()
        : api::OpenSubtitlesClient(/*http=*/nullptr,
              m_apiKey, m_username, m_password, nullptr)
    {
        m_apiKey = QStringLiteral("key");
        m_username = QStringLiteral("user");
        m_password = QStringLiteral("pass");
    }

    QCoro::Task<QList<api::SubtitleHit>> search(api::SubtitleSearchQuery q) override
    {
        searchCount++;
        lastQuery = q;
        if (failNextSearch) {
            failNextSearch = false;
            throw core::HttpError(core::HttpError::Kind::HttpStatus, 500,
                QStringLiteral("simulated failure"));
        }
        co_return scriptedHits;
    }

    QCoro::Task<void> ensureLoggedIn() override { co_return; }

    QCoro::Task<api::SubtitleDownloadTicket> requestDownload(QString fileId) override
    {
        Q_UNUSED(fileId);
        downloadCount++;
        if (quotaExhausted) {
            api::SubtitleDownloadTicket t;
            t.remaining = 0;
            t.resetAt = QDateTime::currentDateTimeUtc().addSecs(3600);
            co_return t;
        }
        api::SubtitleDownloadTicket t;
        t.link = QUrl(QStringLiteral("https://example.com/dl"));
        t.fileName = QStringLiteral("download.eng.srt");
        t.format = QStringLiteral("srt");
        t.remaining = 19;
        co_return t;
    }

    QCoro::Task<QByteArray> fetchFileBytes(QUrl link) override
    {
        Q_UNUSED(link);
        co_return QByteArrayLiteral("1\n00:00:01,000 --> 00:00:02,000\nHello\n\n");
    }

    QList<api::SubtitleHit> scriptedHits;
    api::SubtitleSearchQuery lastQuery;
    int searchCount = 0;
    int downloadCount = 0;
    bool failNextSearch = false;
    bool quotaExhausted = false;

private:
    QString m_apiKey;
    QString m_username;
    QString m_password;
};

api::SubtitleHit makeHit(const QString& id, const QString& lang,
    int downloads = 100, bool moviehash = false)
{
    api::SubtitleHit h;
    h.fileId = id;
    h.language = lang;
    h.languageName = lang == QStringLiteral("eng")
        ? QStringLiteral("English")
        : QStringLiteral("Other");
    h.releaseName = QStringLiteral("Some.Release.") + id;
    h.fileName = QStringLiteral("subtitle.") + id + QStringLiteral(".srt");
    h.format = QStringLiteral("srt");
    h.downloadCount = downloads;
    h.moviehashMatch = moviehash;
    return h;
}

api::PlaybackKey movieKey(const QString& imdb)
{
    api::PlaybackKey k;
    k.kind = api::MediaKind::Movie;
    k.imdbId = imdb;
    return k;
}

void drain()
{
    for (int i = 0; i < 50; ++i) {
        QCoreApplication::processEvents(QEventLoop::AllEvents, 5);
    }
}

} // namespace

class TstSubtitleController : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void initTestCase()
    {
        QStandardPaths::setTestModeEnabled(true);
    }

    void init()
    {
        m_tmp = std::make_unique<QTemporaryDir>();
        QVERIFY(m_tmp->isValid());
        m_db = std::make_unique<core::Database>(
            m_tmp->filePath(QStringLiteral("kinema.db")), nullptr);
        QVERIFY(m_db->open());
        m_cache = std::make_unique<core::SubtitleCacheStore>(*m_db);
        m_config = KSharedConfig::openConfig(
            m_tmp->filePath(QStringLiteral("kinemarc")));
        m_settings = std::make_unique<config::AppSettings>(m_config, nullptr);
        m_client = std::make_unique<FakeOpenSubtitles>();
        m_controller = std::make_unique<controllers::SubtitleController>(
            m_client.get(), m_cache.get(),
            m_settings->subtitle(), m_settings->cache());
    }

    void cleanup()
    {
        m_controller.reset();
        m_client.reset();
        m_settings.reset();
        m_config.reset();
        m_cache.reset();
        m_db.reset();
        m_tmp.reset();
    }

    // ---- runQuery sorts hash-matched first, ties by download count -------

    void testSortsHashMatchedFirst()
    {
        m_client->scriptedHits = {
            makeHit(QStringLiteral("F1"), QStringLiteral("eng"), 1000, false),
            makeHit(QStringLiteral("F2"), QStringLiteral("eng"), 50, true),
            makeHit(QStringLiteral("F3"), QStringLiteral("eng"), 500, true),
        };
        QSignalSpy spy(m_controller.get(),
            &controllers::SubtitleController::hitsChanged);

        m_controller->runQuery(movieKey(QStringLiteral("tt0133093")),
            {}, QStringLiteral("off"), QStringLiteral("off"), QString {});
        drain();

        QVERIFY(spy.count() >= 1);
        const auto hits = m_controller->hits();
        QCOMPARE(hits.size(), 3);
        QCOMPARE(hits.at(0).fileId, QStringLiteral("F3")); // hash + 500
        QCOMPARE(hits.at(1).fileId, QStringLiteral("F2")); // hash + 50
        QCOMPARE(hits.at(2).fileId, QStringLiteral("F1")); // no hash + 1000
    }

    // ---- moviehash forwards into the next search ------------------------

    void testMoviehashForwardedToQuery()
    {
        m_controller->setMoviehash(QStringLiteral("0123456789abcdef"));
        m_client->scriptedHits = { makeHit(QStringLiteral("F1"),
            QStringLiteral("eng")) };
        m_controller->runQuery(movieKey(QStringLiteral("tt0133093")),
            {}, QStringLiteral("off"), QStringLiteral("off"), QString {});
        drain();
        QCOMPARE(m_client->lastQuery.moviehash,
            QStringLiteral("0123456789abcdef"));
    }

    // ---- Stale (epoch) responses are dropped ----------------------------

    void testStaleResponsesDropped()
    {
        m_client->scriptedHits = { makeHit(QStringLiteral("OLD"),
            QStringLiteral("eng")) };
        m_controller->runQuery(movieKey(QStringLiteral("tt0133093")),
            {}, QStringLiteral("off"), QStringLiteral("off"), QString {});
        // Immediately fire a second query before the first coroutine
        // resumes. With our co_await-on-an-immediate task, both resolve
        // synchronously \u2014 but the epoch check still drops the first.
        m_client->scriptedHits = { makeHit(QStringLiteral("NEW"),
            QStringLiteral("eng")) };
        m_controller->runQuery(movieKey(QStringLiteral("tt0133093")),
            {}, QStringLiteral("off"), QStringLiteral("off"), QString {});
        drain();

        const auto hits = m_controller->hits();
        QCOMPARE(hits.size(), 1);
        QCOMPARE(hits.first().fileId, QStringLiteral("NEW"));
    }

    // ---- Cache hit short-circuits the network --------------------------

    void testCacheHitSkipsNetwork()
    {
        // Pre-populate the cache.
        const auto path = m_tmp->filePath(QStringLiteral("cached.srt"));
        QFile f(path);
        QVERIFY(f.open(QIODevice::WriteOnly));
        f.write(QByteArrayLiteral("cached"));
        f.close();

        core::SubtitleCacheStore::Entry e;
        e.fileId = QStringLiteral("F-cached");
        e.imdbId = QStringLiteral("tt0133093");
        e.language = QStringLiteral("eng");
        e.languageName = QStringLiteral("English");
        e.format = QStringLiteral("srt");
        e.localPath = path;
        e.sizeBytes = 6;
        e.addedAt = QDateTime::currentDateTimeUtc();
        e.lastUsedAt = e.addedAt;
        QVERIFY(m_cache->insert(e));

        QSignalSpy finished(m_controller.get(),
            &controllers::SubtitleController::downloadFinished);
        m_controller->download(QStringLiteral("F-cached"),
            movieKey(QStringLiteral("tt0133093")));
        drain();

        QCOMPARE(finished.count(), 1);
        QCOMPARE(m_client->downloadCount, 0);
    }

    // ---- Quota-exhausted response surfaces a reset-time message ---------

    void testQuotaExhaustedMessage()
    {
        m_client->quotaExhausted = true;
        QSignalSpy failed(m_controller.get(),
            &controllers::SubtitleController::downloadFailed);

        m_controller->download(QStringLiteral("F-quota"),
            movieKey(QStringLiteral("tt0133093")));
        drain();

        QCOMPARE(failed.count(), 1);
        const auto reason = failed.first().at(1).toString();
        QVERIFY(reason.contains(QStringLiteral("quota"), Qt::CaseInsensitive));
    }

    // ---- Reconcile drops orphan rows ------------------------------------

    void testReconcileDropsOrphanRows()
    {
        core::SubtitleCacheStore::Entry e;
        e.fileId = QStringLiteral("F-ghost");
        e.imdbId = QStringLiteral("tt0133093");
        e.language = QStringLiteral("eng");
        e.languageName = QStringLiteral("English");
        e.format = QStringLiteral("srt");
        e.localPath = QStringLiteral("/nonexistent/path/sub.srt");
        e.sizeBytes = 100;
        e.addedAt = QDateTime::currentDateTimeUtc();
        e.lastUsedAt = e.addedAt;
        QVERIFY(m_cache->insert(e));

        QCOMPARE(m_cache->all().size(), 1);
        m_controller->reconcileCacheOnStartup();
        drain();
        QCOMPARE(m_cache->all().size(), 0);
    }

    // ---- Search error surfaces via lastError + hits cleared -------------

    void testSearchErrorSurfaces()
    {
        m_client->failNextSearch = true;
        QSignalSpy errSpy(m_controller.get(),
            &controllers::SubtitleController::errorChanged);

        m_controller->runQuery(movieKey(QStringLiteral("tt0133093")),
            {}, QStringLiteral("off"), QStringLiteral("off"), QString {});
        drain();
        QVERIFY(errSpy.count() >= 1);
        QVERIFY(!m_controller->lastError().isEmpty());
        QCOMPARE(m_controller->hits().size(), 0);
    }

private:
    std::unique_ptr<QTemporaryDir> m_tmp;
    std::unique_ptr<core::Database> m_db;
    std::unique_ptr<core::SubtitleCacheStore> m_cache;
    KSharedConfigPtr m_config;
    std::unique_ptr<config::AppSettings> m_settings;
    std::unique_ptr<FakeOpenSubtitles> m_client;
    std::unique_ptr<controllers::SubtitleController> m_controller;
};

QTEST_MAIN(TstSubtitleController)
#include "tst_subtitle_controller.moc"
