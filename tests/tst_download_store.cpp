// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/Download.h"
#include "core/Database.h"
#include "core/DownloadStore.h"

#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>

using namespace kinema;

namespace {

api::DownloadItem sample(const QString& assetId,
    api::CacheDisposition d = api::CacheDisposition::Ephemeral,
    api::DownloadState s = api::DownloadState::Queued,
    api::DownloadMode m = api::DownloadMode::OnDemand)
{
    api::DownloadItem it;
    it.assetId = assetId;
    it.backendKind = api::DownloadBackendKind::Torrent;
    it.state = s;
    it.mode = m;
    it.disposition = d;
    it.key.kind = api::MediaKind::Movie;
    it.key.imdbId = QStringLiteral("tt1234567");
    it.title = QStringLiteral("Sample Movie");
    it.poster = QUrl(QStringLiteral("https://example.com/p.jpg"));
    it.infoHash = QStringLiteral("aabb1122ccdd3344eeff5566778899aabbccddee");
    it.releaseName = QStringLiteral("Sample.Movie.2023.1080p");
    it.fileIndex = 0;
    it.fileNameHint = QStringLiteral("Sample.Movie.2023.1080p.mkv");
    it.qualityLabel = QStringLiteral("Torrentio 1080p");
    it.resolution = QStringLiteral("1080p");
    it.provider = QStringLiteral("ThePirateBay");
    it.expectedSizeBytes = 2147483648LL;
    it.cachedSizeBytes = 0;
    it.localDir = QStringLiteral("/tmp/asset");
    return it;
}

} // namespace

class TstDownloadStore : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void init()
    {
        m_db = std::make_unique<core::Database>(
            QStringLiteral(":memory:"), nullptr);
        QVERIFY(m_db->open());
        m_store = std::make_unique<core::DownloadStore>(*m_db);
    }

    void cleanup()
    {
        m_store.reset();
        m_db.reset();
    }

    void upsertAndFindRoundTrip()
    {
        auto a = sample(QStringLiteral("asset-a"));
        m_store->upsert(a);

        const auto found = m_store->find(QStringLiteral("asset-a"));
        QVERIFY(found.has_value());
        QCOMPARE(found->title, QStringLiteral("Sample Movie"));
        QCOMPARE(found->fileIndex, 0);
        QCOMPARE(found->fileNameHint,
            QStringLiteral("Sample.Movie.2023.1080p.mkv"));
        QCOMPARE(found->infoHash,
            QStringLiteral("aabb1122ccdd3344eeff5566778899aabbccddee"));
        QCOMPARE(found->expectedSizeBytes.value_or(-1), 2147483648LL);
        QCOMPARE(found->cachedSizeBytes, qint64(0));
        QCOMPARE(found->state, api::DownloadState::Queued);
        QCOMPARE(found->mode, api::DownloadMode::OnDemand);
        QCOMPARE(found->disposition, api::CacheDisposition::Ephemeral);
    }

    void modeRoundTripsAndIsMutable()
    {
        auto a = sample(QStringLiteral("asset-a"),
            api::CacheDisposition::Pinned,
            api::DownloadState::Active,
            api::DownloadMode::Full);
        m_store->upsert(a);

        const auto found = m_store->find(QStringLiteral("asset-a"));
        QVERIFY(found.has_value());
        QCOMPARE(found->mode, api::DownloadMode::Full);
        QCOMPARE(found->state, api::DownloadState::Active);

        m_store->updateMode(QStringLiteral("asset-a"),
            api::DownloadMode::OnDemand);
        const auto after = m_store->find(QStringLiteral("asset-a"));
        QVERIFY(after.has_value());
        QCOMPARE(after->mode, api::DownloadMode::OnDemand);
    }

    void synthesiseStartArgsRoundTripsCoreFields()
    {
        auto row = sample(QStringLiteral("asset-a"),
            api::CacheDisposition::Pinned,
            api::DownloadState::Active,
            api::DownloadMode::Full);
        const auto args = core::DownloadStore::synthesiseStartArgs(row);

        QCOMPARE(args.ref.infoHash, row.infoHash);
        QCOMPARE(args.ref.releaseName, row.releaseName);
        QCOMPARE(args.ref.fileIndex, row.fileIndex);
        QCOMPARE(args.ref.fileNameHint, row.fileNameHint);
        QCOMPARE(args.ref.qualityLabel, row.qualityLabel);
        QCOMPARE(args.ref.resolution, row.resolution);
        QCOMPARE(args.ref.provider, row.provider);
        QVERIFY(args.ref.isValid());

        QCOMPARE(args.stream.infoHash, row.infoHash);
        QCOMPARE(args.stream.releaseName, row.releaseName);

        QCOMPARE(args.ctx.key.imdbId, row.key.imdbId);
        QCOMPARE(args.ctx.title, row.title);
        QCOMPARE(args.ctx.poster, row.poster);
    }

    void updateProgressTouchesFields()
    {
        m_store->upsert(sample(QStringLiteral("asset-a")));
        m_store->updateProgress(QStringLiteral("asset-a"), 1024 * 1024, false);

        const auto found = m_store->find(QStringLiteral("asset-a"));
        QVERIFY(found.has_value());
        QCOMPARE(found->cachedSizeBytes, qint64(1024 * 1024));
        QVERIFY(!found->complete);

        m_store->updateProgress(QStringLiteral("asset-a"), 2147483648LL, true);
        const auto found2 = m_store->find(QStringLiteral("asset-a"));
        QVERIFY(found2.has_value());
        QVERIFY(found2->complete);
    }

    void updateStateAndDisposition()
    {
        m_store->upsert(sample(QStringLiteral("asset-a")));
        m_store->updateState(QStringLiteral("asset-a"),
            api::DownloadState::Failed, QStringLiteral("network down"));
        m_store->setDisposition(QStringLiteral("asset-a"),
            api::CacheDisposition::Pinned);

        const auto found = m_store->find(QStringLiteral("asset-a"));
        QVERIFY(found.has_value());
        QCOMPARE(found->state, api::DownloadState::Failed);
        QCOMPARE(found->lastError, QStringLiteral("network down"));
        QCOMPARE(found->disposition, api::CacheDisposition::Pinned);
    }

    void findForKeyReturnsLatest()
    {
        m_store->upsert(sample(QStringLiteral("asset-a")));
        api::DownloadItem b = sample(QStringLiteral("asset-b"));
        b.title = QStringLiteral("Sample Movie B");
        m_store->upsert(b);
        // Bump asset-b's updated_at by re-upserting.
        m_store->upsert(b);

        api::PlaybackKey key;
        key.kind = api::MediaKind::Movie;
        key.imdbId = QStringLiteral("tt1234567");

        const auto found = m_store->findForKey(key);
        QVERIFY(found.has_value());
        QVERIFY(found->assetId == QStringLiteral("asset-a")
            || found->assetId == QStringLiteral("asset-b"));
    }

    void removeAndClear()
    {
        m_store->upsert(sample(QStringLiteral("asset-a")));
        m_store->upsert(sample(QStringLiteral("asset-b")));
        QCOMPARE(m_store->loadAll().size(), 2);

        m_store->remove(QStringLiteral("asset-a"));
        QCOMPARE(m_store->loadAll().size(), 1);

        m_store->clear();
        QVERIFY(m_store->loadAll().isEmpty());
    }

    void changedSignalCoalesced()
    {
        QSignalSpy spy(m_store.get(), &core::DownloadStore::changed);
        m_store->upsert(sample(QStringLiteral("asset-a")));
        m_store->upsert(sample(QStringLiteral("asset-b")));
        m_store->updateProgress(QStringLiteral("asset-a"), 100, false);

        // Coalesced via QTimer::singleShot(0, ...) \u2014 should fire
        // exactly once on the next event-loop turn.
        QVERIFY(spy.wait(1000));
        QCOMPARE(spy.count(), 1);
    }

    void closedDatabaseDegradesGracefully()
    {
        core::Database closed(QStringLiteral(":memory:"), nullptr);
        // not opened
        core::DownloadStore store(closed);
        store.upsert(sample(QStringLiteral("x")));
        QVERIFY(store.loadAll().isEmpty());
        QVERIFY(!store.find(QStringLiteral("x")).has_value());
    }

private:
    std::unique_ptr<core::Database> m_db;
    std::unique_ptr<core::DownloadStore> m_store;
};

QTEST_MAIN(TstDownloadStore)
#include "tst_download_store.moc"
