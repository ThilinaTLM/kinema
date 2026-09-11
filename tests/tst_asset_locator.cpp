// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/io/CachePaths.h"
#include "core/persistence/AssetLocator.h"

#include <QDir>
#include <QFile>
#include <QStandardPaths>
#include <QTest>

using namespace kinema;

namespace {

QString writeFile(const QDir& dir, const QString& relPath, qint64 bytes)
{
    const auto path = dir.absoluteFilePath(relPath);
    QDir().mkpath(QFileInfo(path).absolutePath());
    QFile f(path);
    if (!f.open(QIODevice::WriteOnly)) {
        return {};
    }
    f.write(QByteArray(static_cast<int>(bytes), 'x'));
    f.close();
    return path;
}

domain::DownloadItem torrentRow(const QString& hash)
{
    domain::DownloadItem it;
    it.assetId = hash + QStringLiteral("-f0");
    it.backendKind = domain::DownloadBackendKind::Torrent;
    it.infoHash = hash;
    // The buggy value the column used to carry: the bookkeeping dir,
    // which never holds media for a torrent row.
    it.localDir = core::cache::mediaDir().absoluteFilePath(it.assetId);
    return it;
}

} // namespace

class TstAssetLocator : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void initTestCase() { QStandardPaths::setTestModeEnabled(true); }

    /// The regression: a torrent row must resolve into the tree
    /// libtorrent actually writes to, not `MediaCache::assetDir`.
    void testTorrentRowResolvesToTorrentTree()
    {
        const QString hash(QStringLiteral("aaaa0000bbbb1111cccc"));
        const auto row = torrentRow(hash);

        const auto dir = core::locateAssetDir(row);
        QCOMPARE(QDir(dir).absolutePath(),
            core::cache::torrentsDir().absoluteFilePath(hash));
        QVERIFY(dir != row.localDir);
    }

    void testHttpRowStaysInMediaTree()
    {
        domain::DownloadItem row;
        row.assetId = QStringLiteral("http-asset-1");
        row.backendKind = domain::DownloadBackendKind::RealDebridHttp;

        const auto dir = core::locateAssetDir(row);
        QCOMPARE(QDir(dir).absolutePath(),
            core::cache::mediaDir().absoluteFilePath(row.assetId));
    }

    void testInfoHashIsNormalised()
    {
        domain::DownloadItem row;
        row.assetId = QStringLiteral("x");
        row.backendKind = domain::DownloadBackendKind::Torrent;
        row.infoHash = QStringLiteral("  AABBCC  ");

        QCOMPARE(QDir(core::locateAssetDir(row)).absolutePath(),
            core::cache::torrentsDir().absoluteFilePath(
                QStringLiteral("aabbcc")));
    }

    /// A single-file torrent needs no hint to be pinned down.
    void testSoleFileIsPicked()
    {
        const QString hash(QStringLiteral("d000000000000000single"));
        auto row = torrentRow(hash);
        QDir dir(core::cache::torrentsDir().absoluteFilePath(hash));
        QDir().mkpath(dir.absolutePath());

        const auto movie = writeFile(dir,
            QStringLiteral("Some.Movie.2021.mkv"), 2048);
        // Cache bookkeeping must never be mistaken for payload.
        writeFile(dir, QStringLiteral(".last-used"), 24);

        const auto loc = core::locateAsset(row);
        QVERIFY(loc.exists);
        QCOMPARE(loc.file, movie);
        QCOMPARE(loc.bestPath(), movie);
    }

    /// A season pack: many files in one directory, disambiguated by
    /// the indexer's file-name hint.
    void testPackDisambiguatedByNameHint()
    {
        const QString hash(QStringLiteral("e000000000000000pack01"));
        auto row = torrentRow(hash);
        QDir dir(core::cache::torrentsDir().absoluteFilePath(hash));
        QDir().mkpath(dir.absolutePath());

        writeFile(dir, QStringLiteral("Show/S01E01.mkv"), 1000);
        const auto want = writeFile(dir,
            QStringLiteral("Show/S01E02.mkv"), 2000);
        writeFile(dir, QStringLiteral("Show/S01E03.mkv"), 3000);

        row.fileNameHint = QStringLiteral("S01E02.mkv");
        const auto loc = core::locateAsset(row);
        QCOMPARE(loc.file, want);
    }

    /// No hint, but the row's expected size is unique in the pack.
    void testPackDisambiguatedByExactSize()
    {
        const QString hash(QStringLiteral("e000000000000000pack02"));
        auto row = torrentRow(hash);
        QDir dir(core::cache::torrentsDir().absoluteFilePath(hash));
        QDir().mkpath(dir.absolutePath());

        writeFile(dir, QStringLiteral("Show/S01E01.mkv"), 1000);
        const auto want = writeFile(dir,
            QStringLiteral("Show/S01E02.mkv"), 2222);

        row.expectedSizeBytes = 2222;
        const auto loc = core::locateAsset(row);
        QCOMPARE(loc.file, want);
    }

    /// Ambiguous stays ambiguous: opening the directory beats
    /// selecting a confidently wrong episode.
    void testAmbiguousPackFallsBackToDirectory()
    {
        const QString hash(QStringLiteral("e000000000000000pack03"));
        auto row = torrentRow(hash);
        QDir dir(core::cache::torrentsDir().absoluteFilePath(hash));
        QDir().mkpath(dir.absolutePath());

        writeFile(dir, QStringLiteral("Show/S01E01.mkv"), 1500);
        writeFile(dir, QStringLiteral("Show/S01E02.mkv"), 1500);

        const auto loc = core::locateAsset(row);
        QVERIFY(loc.exists);
        QVERIFY(loc.file.isEmpty());
        QCOMPARE(QDir(loc.bestPath()).absolutePath(),
            dir.absolutePath());
    }

    /// A row whose directory was evicted reports `exists == false` so
    /// the caller can say so instead of failing silently.
    void testMissingDirectoryIsReported()
    {
        auto row = torrentRow(QStringLiteral("f000000000000000gone01"));
        const auto loc = core::locateAsset(row);
        QVERIFY(!loc.dir.isEmpty());
        QVERIFY(!loc.exists);
        QVERIFY(loc.file.isEmpty());
    }
};

QTEST_MAIN(TstAssetLocator)
#include "tst_asset_locator.moc"
