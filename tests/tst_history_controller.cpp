// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "TestDoubles.h"

#include "api/PlaybackContext.h"
#include "config/AppSettings.h"
#include "controllers/HistoryController.h"
#include "core/Database.h"
#include "core/HistoryStore.h"
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
        api::Stream stream;
        api::PlaybackContext ctx;
    };

    QList<Call> calls;

    void play(const api::Stream& stream,
        const api::PlaybackContext& ctx) override
    {
        calls.append({ stream, ctx });
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
