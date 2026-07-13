// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/TorrentStreamingSettings.h"
#include "core/io/CachePaths.h"
#include "core/persistence/TorrentCache.h"

#include <KSharedConfig>

#include <QDir>
#include <QFile>
#include <QSet>
#include <QStandardPaths>
#include <QTest>

#include <memory>

using namespace kinema;

namespace {

void writeFile(const QString& path, qint64 sizeBytes)
{
    QFile f(path);
    QVERIFY(f.open(QIODevice::WriteOnly));
    f.write(QByteArray(static_cast<int>(sizeBytes), 'x'));
    f.close();
}

} // namespace

class TstTorrentCache : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void initTestCase()
    {
        QStandardPaths::setTestModeEnabled(true);
    }

    void init()
    {
        QDir(core::cache::torrentsDir()).removeRecursively();
        QDir().mkpath(core::cache::torrentsDir().absolutePath());

        m_config = KSharedConfig::openConfig(
            QStringLiteral("kinemarc-torrentcache-test"),
            KConfig::SimpleConfig);
        m_settings = std::make_unique<config::TorrentStreamingSettings>(
            m_config);
        m_cache = std::make_unique<core::TorrentCache>(*m_settings);
    }

    void cleanup()
    {
        m_cache.reset();
        m_settings.reset();
        QDir(core::cache::torrentsDir()).removeRecursively();
    }

    void removeAllExceptKeepsProtectedHashes()
    {
        const auto protectedHash = QStringLiteral("ABCDEF1234");
        const auto unprotectedHash = QStringLiteral("1234ABCDEF");

        const auto protectedPayload = m_cache->torrentDir(protectedHash)
                                          .absoluteFilePath(QStringLiteral("payload"));
        const auto unprotectedPayload = m_cache->torrentDir(unprotectedHash)
                                            .absoluteFilePath(QStringLiteral("payload"));
        writeFile(protectedPayload, 128);
        writeFile(unprotectedPayload, 128);

        const auto result = m_cache->removeAllExcept(
            QSet<QString> { protectedHash });

        QCOMPARE(result.failedTorrents, 0);
        QCOMPARE(result.removedTorrents, 1);
        QVERIFY(QFile::exists(protectedPayload));
        QVERIFY(!QFile::exists(unprotectedPayload));
    }

    void removeAllExceptSkipsActiveUnprotectedHashes()
    {
        const auto activeHash = QStringLiteral("abcdef9999");
        const auto activePayload = m_cache->torrentDir(activeHash)
                                       .absoluteFilePath(QStringLiteral("payload"));
        writeFile(activePayload, 128);
        m_cache->markActive(activeHash);

        const auto result = m_cache->removeAllExcept(QSet<QString> {});

        QCOMPARE(result.failedTorrents, 0);
        QCOMPARE(result.removedTorrents, 0);
        QVERIFY(QFile::exists(activePayload));
    }

private:
    KSharedConfigPtr m_config;
    std::unique_ptr<config::TorrentStreamingSettings> m_settings;
    std::unique_ptr<core::TorrentCache> m_cache;
};

QTEST_MAIN(TstTorrentCache)
#include "tst_torrent_cache.moc"
