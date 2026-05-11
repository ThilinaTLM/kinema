// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/Indexer.h"
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

    void defaults_activeIsTorrentioAndTorrentioConfigured()
    {
        config::IndexerSettings s(m_config);
        QCOMPARE(s.activeIndexer(), api::IndexerKind::Torrentio);
        // Torrentio works without setup so it defaults to configured.
        QVERIFY(s.torrentioConfigured());
        QVERIFY(!s.mediaFusionConfigured());
    }

    // ---- Roundtrip ------------------------------------------------------

    void roundtrip_active_persistsAcrossInstances()
    {
        {
            config::IndexerSettings s(m_config);
            s.setActiveIndexer(api::IndexerKind::MediaFusion);
        }
        m_config->sync();

        auto cfg2 = KSharedConfig::openConfig(
            m_configPath, KConfig::SimpleConfig);
        config::IndexerSettings s2(cfg2);
        QCOMPARE(s2.activeIndexer(), api::IndexerKind::MediaFusion);
    }

    void roundtrip_configuredFlags_persistAcrossInstances()
    {
        {
            config::IndexerSettings s(m_config);
            s.setMediaFusionConfigured(true);
            s.setTorrentioConfigured(false);
        }
        m_config->sync();

        auto cfg2 = KSharedConfig::openConfig(
            m_configPath, KConfig::SimpleConfig);
        config::IndexerSettings s2(cfg2);
        QVERIFY(s2.mediaFusionConfigured());
        QVERIFY(!s2.torrentioConfigured());
    }

    // ---- Signals --------------------------------------------------------

    void setActiveIndexer_emitsOnlyOnChange()
    {
        config::IndexerSettings s(m_config);
        QSignalSpy spy(&s, &config::IndexerSettings::activeIndexerChanged);

        s.setActiveIndexer(api::IndexerKind::Torrentio); // already default
        QCOMPARE(spy.count(), 0);

        s.setActiveIndexer(api::IndexerKind::MediaFusion);
        QCOMPARE(spy.count(), 1);
        QCOMPARE(spy.at(0).at(0).value<api::IndexerKind>(),
            api::IndexerKind::MediaFusion);

        s.setActiveIndexer(api::IndexerKind::MediaFusion); // no-op
        QCOMPARE(spy.count(), 1);
    }

    void setMediaFusionConfigured_emitsOnlyOnChange()
    {
        config::IndexerSettings s(m_config);
        QSignalSpy spy(&s,
            &config::IndexerSettings::mediaFusionConfiguredChanged);

        s.setMediaFusionConfigured(false); // already default
        QCOMPARE(spy.count(), 0);

        s.setMediaFusionConfigured(true);
        QCOMPARE(spy.count(), 1);
        QCOMPARE(spy.at(0).at(0).toBool(), true);
    }

    // ---- Enum tokens ----------------------------------------------------

    void indexerKindToString_roundTrips()
    {
        for (const auto k :
            { api::IndexerKind::Torrentio, api::IndexerKind::MediaFusion }) {
            QCOMPARE(api::indexerKindFromString(api::indexerKindToString(k)),
                k);
        }
    }

    void indexerKindFromString_defaultsToTorrentio()
    {
        QCOMPARE(api::indexerKindFromString(QString {}),
            api::IndexerKind::Torrentio);
        QCOMPARE(api::indexerKindFromString(QStringLiteral("bogus")),
            api::IndexerKind::Torrentio);
        QCOMPARE(api::indexerKindFromString(QStringLiteral("  MediaFusion  ")),
            api::IndexerKind::MediaFusion);
    }

private:
    std::unique_ptr<QTemporaryDir> m_tmpdir;
    QString m_configPath;
    KSharedConfigPtr m_config;
};

QTEST_GUILESS_MAIN(TstIndexerSettings)
#include "tst_indexer_settings.moc"
