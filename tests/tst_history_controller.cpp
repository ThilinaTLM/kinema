// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "TestDoubles.h"

#include "api/PlaybackContext.h"
#include "config/AppSettings.h"
#include "controllers/HistoryController.h"
#include "controllers/PlayQueueController.h"
#include "core/Database.h"
#include "core/HistoryStore.h"
#include "core/PlayQueueStore.h"
#include "services/StreamActions.h"

#include <KConfig>
#include <KSharedConfig>

#include <QDateTime>
#include <QTemporaryDir>
#include <QTest>

using namespace kinema;
using kinema::tests::FakeTorrentioClient;
using kinema::tests::drainEvents;

namespace {

class RecordingStreamActions : public services::StreamActions
{
public:
    explicit RecordingStreamActions(QObject* parent = nullptr)
        : services::StreamActions(/*launcher=*/nullptr, /*torrentStreaming=*/nullptr, parent)
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

api::HistoryEntry makeHistoryEntry(const QString& imdb,
    const QString& title, const QString& releaseSuffix)
{
    api::HistoryEntry e;
    e.key.kind = api::MediaKind::Movie;
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

api::Stream makeStream(const QString& releaseSuffix,
    const QString& directUrl)
{
    api::Stream s;
    s.infoHash = QStringLiteral("hash-") + releaseSuffix;
    s.releaseName = QStringLiteral("Release.") + releaseSuffix;
    s.resolution = QStringLiteral("1080p");
    s.qualityLabel = QStringLiteral("Torrentio 1080p");
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
        m_queueStore = std::make_unique<core::PlayQueueStore>(*m_db);
        m_config = KSharedConfig::openConfig(
            m_tmp->filePath(QStringLiteral("kinemarc")),
            KConfig::SimpleConfig);
        m_settings = std::make_unique<config::AppSettings>(m_config);
        m_torrentio = std::make_unique<FakeTorrentioClient>();
        m_actions = std::make_unique<RecordingStreamActions>();
        m_historyCtrl = std::make_unique<controllers::HistoryController>(
            *m_historyStore, m_torrentio.get(), *m_settings, m_rdToken);
        m_queueCtrl = std::make_unique<controllers::PlayQueueController>(
            *m_queueStore, *m_torrentio, *m_actions, *m_settings,
            m_rdToken);
        m_historyCtrl->setPlayQueue(m_queueCtrl.get());
    }

    void cleanup()
    {
        m_queueCtrl.reset();
        m_historyCtrl.reset();
        m_actions.reset();
        m_torrentio.reset();
        m_settings.reset();
        m_config.reset();
        m_queueStore.reset();
        m_historyStore.reset();
        m_db.reset();
        m_tmp.reset();
    }

    void resumeRoutesThroughQueueAndDeduplicates()
    {
        m_queueCtrl->enqueue(makeStream(QStringLiteral("OLD"),
                                 QStringLiteral("https://rd.example/old")),
            makeMovieCtx(QStringLiteral("ttdup"), QStringLiteral("Movie")));
        m_queueCtrl->enqueue(makeStream(QStringLiteral("OTHER"),
                                 QStringLiteral("https://rd.example/other")),
            makeMovieCtx(QStringLiteral("ttother"), QStringLiteral("Other")));
        QCOMPARE(m_queueCtrl->items().size(), 2);
        QCOMPARE(m_actions->calls.size(), 0);

        FakeTorrentioClient::ScriptedCall sc;
        sc.suspend = true;
        sc.streams.append(makeStream(QStringLiteral("NEW"),
            QStringLiteral("https://rd.example/new")));
        m_torrentio->scriptedCalls.append(sc);

        const auto entry = makeHistoryEntry(QStringLiteral("ttdup"),
            QStringLiteral("Movie"), QStringLiteral("NEW"));
        m_historyCtrl->resumeFromHistory(entry);
        drainEvents(4);

        QCOMPARE(m_torrentio->callCount, 1);
        QCOMPARE(m_queueCtrl->items().size(), 2);
        QCOMPARE(m_queueCtrl->activeIndex(), 0);
        QCOMPARE(m_queueCtrl->items()[0].key.imdbId,
            QStringLiteral("ttdup"));
        QCOMPARE(m_queueCtrl->items()[0].streamRef.releaseName,
            QStringLiteral("Release.NEW"));
        QCOMPARE(m_queueCtrl->items()[1].key.imdbId,
            QStringLiteral("ttother"));
        QCOMPARE(m_actions->calls.size(), 1);
        QCOMPARE(m_actions->calls[0].stream.directUrl,
            QUrl(QStringLiteral("https://rd.example/new")));
    }

private:
    std::unique_ptr<QTemporaryDir> m_tmp;
    std::unique_ptr<core::Database> m_db;
    std::unique_ptr<core::HistoryStore> m_historyStore;
    std::unique_ptr<core::PlayQueueStore> m_queueStore;
    KSharedConfig::Ptr m_config;
    std::unique_ptr<config::AppSettings> m_settings;
    std::unique_ptr<FakeTorrentioClient> m_torrentio;
    std::unique_ptr<RecordingStreamActions> m_actions;
    std::unique_ptr<controllers::HistoryController> m_historyCtrl;
    std::unique_ptr<controllers::PlayQueueController> m_queueCtrl;
    QString m_rdToken { QStringLiteral("rd-token-stub") };
};

QTEST_MAIN(TstHistoryController)
#include "tst_history_controller.moc"
