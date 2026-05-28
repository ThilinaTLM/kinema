// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/persistence/Database.h"
#include "core/persistence/DownloadStore.h"
#include "domain/Download.h"
#include "playback/downloads/SqliteDownloadRepository.h"

#include <QTest>

using namespace kinema;

namespace {

domain::DownloadItem mk(const QString& id)
{
    domain::DownloadItem it;
    it.assetId = id;
    it.backendKind = domain::DownloadBackendKind::Torrent;
    it.state = domain::DownloadState::Queued;
    it.mode = domain::DownloadMode::OnDemand;
    it.disposition = domain::CacheDisposition::Ephemeral;
    it.key.kind = domain::MediaKind::Movie;
    it.key.imdbId = QStringLiteral("tt001");
    it.title = QStringLiteral("Sample");
    it.infoHash = QStringLiteral("aabb1122ccdd3344eeff5566778899aabbccddee");
    it.releaseName = QStringLiteral("Sample.Movie.2023");
    it.fileIndex = 0;
    it.expectedSizeBytes = 1024;
    it.localDir = QStringLiteral("/tmp/x");
    return it;
}

} // namespace

class TstSqliteDownloadRepository : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void roundtripsThroughAdapter()
    {
        core::Database db(QStringLiteral(":memory:"), nullptr);
        QVERIFY(db.open());
        core::DownloadStore store(db);

        playback::downloads::SqliteDownloadRepository repo(store);

        repo.upsert(mk(QStringLiteral("asset-a")));
        const auto loaded = repo.find(QStringLiteral("asset-a"));
        QVERIFY(loaded.has_value());
        QCOMPARE(loaded->assetId, QStringLiteral("asset-a"));

        repo.updateCachedBytes(QStringLiteral("asset-a"), 512,
            std::optional<qint64>(1024), false);
        const auto after = repo.find(QStringLiteral("asset-a"));
        QVERIFY(after);
        QCOMPARE(after->cachedSizeBytes, 512);

        repo.updateState(QStringLiteral("asset-a"),
            domain::DownloadState::Completed);
        const auto done = repo.find(QStringLiteral("asset-a"));
        QVERIFY(done);
        QCOMPARE(done->state, domain::DownloadState::Completed);

        repo.remove(QStringLiteral("asset-a"));
        QVERIFY(!repo.find(QStringLiteral("asset-a")).has_value());
    }
};

QTEST_MAIN(TstSqliteDownloadRepository)
#include "tst_sqlite_download_repository.moc"
