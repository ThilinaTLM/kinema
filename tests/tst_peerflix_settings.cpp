// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/PeerflixSettings.h"

#include <KConfig>
#include <KSharedConfig>

#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>

using namespace kinema;

class TstPeerflixSettings : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void init()
    {
        m_tmp = std::make_unique<QTemporaryDir>();
        QVERIFY(m_tmp->isValid());
        m_configPath = m_tmp->filePath(QStringLiteral("kinemarc"));
        m_config = KSharedConfig::openConfig(
            m_configPath, KConfig::SimpleConfig);
    }

    void cleanup()
    {
        m_config.reset();
        m_tmp.reset();
    }

    void defaults_baseUrlIsPublicHost()
    {
        config::PeerflixSettings s(m_config);
        QCOMPARE(s.baseUrl(),
            config::PeerflixSettings::defaultBaseUrl());
        QCOMPARE(s.baseUrl(), QStringLiteral("https://peerflix.mov"));
    }

    void setBaseUrl_emptyResetsToDefault()
    {
        config::PeerflixSettings s(m_config);
        s.setBaseUrl(QStringLiteral("https://mirror.example"));
        QCOMPARE(s.baseUrl(), QStringLiteral("https://mirror.example"));
        s.setBaseUrl(QString {});
        QCOMPARE(s.baseUrl(),
            config::PeerflixSettings::defaultBaseUrl());
    }

    void setBaseUrl_trimsWhitespace()
    {
        config::PeerflixSettings s(m_config);
        s.setBaseUrl(QStringLiteral("   https://x.example   "));
        QCOMPARE(s.baseUrl(), QStringLiteral("https://x.example"));
    }

    void setBaseUrl_emitsOnlyOnChange()
    {
        config::PeerflixSettings s(m_config);
        QSignalSpy spy(&s, &config::PeerflixSettings::baseUrlChanged);

        // Same as default → no emission.
        s.setBaseUrl(config::PeerflixSettings::defaultBaseUrl());
        QCOMPARE(spy.count(), 0);

        s.setBaseUrl(QStringLiteral("https://mirror.example"));
        QCOMPARE(spy.count(), 1);

        // Identical follow-up → no emission.
        s.setBaseUrl(QStringLiteral("https://mirror.example"));
        QCOMPARE(spy.count(), 1);
    }

    void setBaseUrl_persistsAcrossInstances()
    {
        {
            config::PeerflixSettings s(m_config);
            s.setBaseUrl(QStringLiteral("https://mirror.example"));
        }
        m_config->sync();

        auto cfg2 = KSharedConfig::openConfig(
            m_configPath, KConfig::SimpleConfig);
        config::PeerflixSettings s2(cfg2);
        QCOMPARE(s2.baseUrl(), QStringLiteral("https://mirror.example"));
    }

    void defaults_addonBaseUrlIsConfigurableHost()
    {
        config::PeerflixSettings s(m_config);
        QCOMPARE(s.addonBaseUrl(),
            config::PeerflixSettings::defaultAddonBaseUrl());
        QCOMPARE(s.addonBaseUrl(),
            QStringLiteral("https://addon.peerflix.mov"));
    }

    void setAddonBaseUrl_emptyResetsToDefault()
    {
        config::PeerflixSettings s(m_config);
        s.setAddonBaseUrl(
            QStringLiteral("https://addon.mirror.example"));
        QCOMPARE(s.addonBaseUrl(),
            QStringLiteral("https://addon.mirror.example"));
        s.setAddonBaseUrl(QString {});
        QCOMPARE(s.addonBaseUrl(),
            config::PeerflixSettings::defaultAddonBaseUrl());
    }

    void setAddonBaseUrl_emitsOnlyOnChange()
    {
        config::PeerflixSettings s(m_config);
        QSignalSpy spy(&s,
            &config::PeerflixSettings::addonBaseUrlChanged);

        s.setAddonBaseUrl(
            config::PeerflixSettings::defaultAddonBaseUrl());
        QCOMPARE(spy.count(), 0);

        s.setAddonBaseUrl(
            QStringLiteral("https://addon.mirror.example"));
        QCOMPARE(spy.count(), 1);
        s.setAddonBaseUrl(
            QStringLiteral("https://addon.mirror.example"));
        QCOMPARE(spy.count(), 1);
    }

    void baseUrl_andAddonBaseUrl_areIndependent()
    {
        // Overriding one must not bleed into the other — they are
        // distinct upstream hosts and live in distinct KConfig keys.
        config::PeerflixSettings s(m_config);
        s.setBaseUrl(QStringLiteral("https://mirror.example"));
        QCOMPARE(s.addonBaseUrl(),
            config::PeerflixSettings::defaultAddonBaseUrl());
        s.setAddonBaseUrl(
            QStringLiteral("https://addon.mirror.example"));
        QCOMPARE(s.baseUrl(),
            QStringLiteral("https://mirror.example"));
    }

private:
    std::unique_ptr<QTemporaryDir> m_tmp;
    QString m_configPath;
    KSharedConfigPtr m_config;
};

QTEST_GUILESS_MAIN(TstPeerflixSettings)
#include "tst_peerflix_settings.moc"
