// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "domain/Indexer.h"
#include "config/IndexerSettings.h"

#include <KConfig>
#include <KSharedConfig>

#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>

using namespace kinema;

class TstIndexerSettings : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void init()
    {
        m_tmpdir = std::make_unique<QTemporaryDir>();
        QVERIFY(m_tmpdir->isValid());
        m_configPath = m_tmpdir->filePath(QStringLiteral("kinemarc"));
        m_config = KSharedConfig::openConfig(
            m_configPath, KConfig::SimpleConfig);
    }

    void cleanup()
    {
        m_config.reset();
        m_tmpdir.reset();
    }

    // ---- Defaults --------------------------------------------------------

    void defaults_activeIsTorrentio()
    {
        config::IndexerSettings s(m_config);
        QCOMPARE(s.activeIndexer(), domain::IndexerKind::Torrentio);
    }

    // ---- Roundtrip ------------------------------------------------------

    void roundtrip_active_persistsAcrossInstances()
    {
        {
            config::IndexerSettings s(m_config);
            s.setActiveIndexer(domain::IndexerKind::Peerflix);
        }
        m_config->sync();

        auto cfg2 = KSharedConfig::openConfig(
            m_configPath, KConfig::SimpleConfig);
        config::IndexerSettings s2(cfg2);
        QCOMPARE(s2.activeIndexer(), domain::IndexerKind::Peerflix);
    }

    // ---- Signals --------------------------------------------------------

    void setActiveIndexer_emitsOnlyOnChange()
    {
        config::IndexerSettings s(m_config);
        QSignalSpy spy(&s, &config::IndexerSettings::activeIndexerChanged);

        s.setActiveIndexer(domain::IndexerKind::Torrentio); // already default
        QCOMPARE(spy.count(), 0);

        s.setActiveIndexer(domain::IndexerKind::Peerflix);
        QCOMPARE(spy.count(), 1);
        QCOMPARE(spy.at(0).at(0).value<domain::IndexerKind>(),
            domain::IndexerKind::Peerflix);

        s.setActiveIndexer(domain::IndexerKind::Peerflix); // no-op
        QCOMPARE(spy.count(), 1);
    }

    // ---- Enum tokens ----------------------------------------------------

    void indexerKindToString_roundTrips()
    {
        for (const auto k :
            { domain::IndexerKind::Torrentio, domain::IndexerKind::Peerflix }) {
            QCOMPARE(domain::indexerKindFromString(domain::indexerKindToString(k)),
                k);
        }
    }

    void indexerKindFromString_defaultsToTorrentio()
    {
        QCOMPARE(domain::indexerKindFromString(QString {}),
            domain::IndexerKind::Torrentio);
        QCOMPARE(domain::indexerKindFromString(QStringLiteral("bogus")),
            domain::IndexerKind::Torrentio);
        // Legacy "mediafusion" token from old configs → falls back to
        // the default rather than crashing.
        QCOMPARE(domain::indexerKindFromString(QStringLiteral("mediafusion")),
            domain::IndexerKind::Torrentio);
        QCOMPARE(domain::indexerKindFromString(QStringLiteral("  Peerflix  ")),
            domain::IndexerKind::Peerflix);
    }

private:
    std::unique_ptr<QTemporaryDir> m_tmpdir;
    QString m_configPath;
    KSharedConfigPtr m_config;
};

QTEST_GUILESS_MAIN(TstIndexerSettings)
#include "tst_indexer_settings.moc"
