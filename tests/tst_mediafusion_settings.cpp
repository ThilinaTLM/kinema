// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/MediaFusionParse.h"
#include "config/MediaFusionSettings.h"

#include <KConfig>
#include <KSharedConfig>

#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>

using namespace kinema;

class TstMediaFusionSettings : public QObject
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

    // ---- URL parser -----------------------------------------------------

    void parseManifestUrl_acceptsConfiguredUrl()
    {
        const auto parts = api::mediafusion::parseManifestUrl(
            QStringLiteral(
                "https://mediafusion.elfhosted.com/D-abcXYZ/manifest.json"));
        QVERIFY(parts.valid);
        QCOMPARE(parts.baseUrl,
            QStringLiteral("https://mediafusion.elfhosted.com"));
        QCOMPARE(parts.encryptedConfig, QStringLiteral("D-abcXYZ"));
    }

    void parseManifestUrl_acceptsUnconfiguredUrl()
    {
        const auto parts = api::mediafusion::parseManifestUrl(
            QStringLiteral("https://example.com/manifest.json"));
        QVERIFY(parts.valid);
        QCOMPARE(parts.baseUrl, QStringLiteral("https://example.com"));
        QVERIFY(parts.encryptedConfig.isEmpty());
    }

    void parseManifestUrl_preservesPort()
    {
        const auto parts = api::mediafusion::parseManifestUrl(
            QStringLiteral("http://localhost:8080/D-tok/manifest.json"));
        QVERIFY(parts.valid);
        QCOMPARE(parts.baseUrl, QStringLiteral("http://localhost:8080"));
        QCOMPARE(parts.encryptedConfig, QStringLiteral("D-tok"));
    }

    void parseManifestUrl_rejectsMissingManifestSuffix()
    {
        QVERIFY(!api::mediafusion::parseManifestUrl(
            QStringLiteral("https://example.com/"))
                     .valid);
        QVERIFY(!api::mediafusion::parseManifestUrl(
            QStringLiteral("https://example.com/D-tok/configure"))
                     .valid);
    }

    void parseManifestUrl_rejectsMultipleSegmentsBeforeManifest()
    {
        QVERIFY(!api::mediafusion::parseManifestUrl(
            QStringLiteral(
                "https://example.com/a/b/manifest.json"))
                     .valid);
    }

    void parseManifestUrl_rejectsEmpty()
    {
        QVERIFY(!api::mediafusion::parseManifestUrl(QString {}).valid);
        QVERIFY(!api::mediafusion::parseManifestUrl(
            QStringLiteral("not a url")).valid);
    }

    // ---- Stream URL builder --------------------------------------------

    void buildStreamUrl_includesTokenWhenPresent()
    {
        api::mediafusion::ManifestUrlParts parts;
        parts.valid = true;
        parts.baseUrl = QStringLiteral("https://mf.example");
        parts.encryptedConfig = QStringLiteral("D-tok");

        QCOMPARE(api::mediafusion::buildStreamUrl(parts,
                     QStringLiteral("movie"), QStringLiteral("tt0133093")),
            QStringLiteral(
                "https://mf.example/D-tok/stream/movie/tt0133093.json"));
    }

    void buildStreamUrl_omitsTokenWhenEmpty()
    {
        api::mediafusion::ManifestUrlParts parts;
        parts.valid = true;
        parts.baseUrl = QStringLiteral("https://mf.example");
        QCOMPARE(api::mediafusion::buildStreamUrl(parts,
                     QStringLiteral("series"),
                     QStringLiteral("tt0903747:1:1")),
            QStringLiteral(
                "https://mf.example/stream/series/tt0903747:1:1.json"));
    }

    void buildStreamUrl_returnsEmptyForInvalidParts()
    {
        api::mediafusion::ManifestUrlParts parts;
        QVERIFY(api::mediafusion::buildStreamUrl(parts,
            QStringLiteral("movie"), QStringLiteral("tt0"))
                    .isEmpty());
    }

    // ---- KConfig roundtrip ---------------------------------------------

    void defaults_baseUrlIsPublicHostAndTokenEmpty()
    {
        config::MediaFusionSettings s(m_config);
        QCOMPARE(s.baseUrl(),
            config::MediaFusionSettings::defaultBaseUrl());
        QVERIFY(s.encryptedConfig().isEmpty());
        QVERIFY(s.manifestUrl().isEmpty());
    }

    void saveFromManifestUrl_persistsDerivedFields()
    {
        config::MediaFusionSettings s(m_config);
        QVERIFY(s.saveFromManifestUrl(QStringLiteral(
            "https://mf.example/D-tok/manifest.json")));
        m_config->sync();

        auto cfg2 = KSharedConfig::openConfig(
            m_configPath, KConfig::SimpleConfig);
        config::MediaFusionSettings s2(cfg2);
        QCOMPARE(s2.baseUrl(), QStringLiteral("https://mf.example"));
        QCOMPARE(s2.encryptedConfig(), QStringLiteral("D-tok"));
        QCOMPARE(s2.manifestUrl(),
            QStringLiteral("https://mf.example/D-tok/manifest.json"));
    }

    void saveFromManifestUrl_rejectsBadInput_leavesStateUntouched()
    {
        config::MediaFusionSettings s(m_config);
        QVERIFY(s.saveFromManifestUrl(QStringLiteral(
            "https://mf.example/D-tok/manifest.json")));

        QVERIFY(!s.saveFromManifestUrl(QStringLiteral("garbage")));
        QCOMPARE(s.encryptedConfig(), QStringLiteral("D-tok"));
    }

    void saveFromManifestUrl_emitsChangeSignals()
    {
        config::MediaFusionSettings s(m_config);
        QSignalSpy baseSpy(&s,
            &config::MediaFusionSettings::baseUrlChanged);
        QSignalSpy tokenSpy(&s,
            &config::MediaFusionSettings::encryptedConfigChanged);

        QVERIFY(s.saveFromManifestUrl(QStringLiteral(
            "https://mf.example/D-tok/manifest.json")));
        QCOMPARE(baseSpy.count(), 1);
        QCOMPARE(tokenSpy.count(), 1);

        // Same input again \u2192 no extra emissions.
        QVERIFY(s.saveFromManifestUrl(QStringLiteral(
            "https://mf.example/D-tok/manifest.json")));
        QCOMPARE(baseSpy.count(), 1);
        QCOMPARE(tokenSpy.count(), 1);
    }

    void clear_resetsAllFieldsAndEmits()
    {
        config::MediaFusionSettings s(m_config);
        QVERIFY(s.saveFromManifestUrl(QStringLiteral(
            "https://mf.example/D-tok/manifest.json")));

        QSignalSpy manifestSpy(&s,
            &config::MediaFusionSettings::manifestUrlChanged);
        QSignalSpy tokenSpy(&s,
            &config::MediaFusionSettings::encryptedConfigChanged);

        s.clear();
        QVERIFY(s.manifestUrl().isEmpty());
        QVERIFY(s.encryptedConfig().isEmpty());
        QCOMPARE(s.baseUrl(),
            config::MediaFusionSettings::defaultBaseUrl());
        QCOMPARE(manifestSpy.count(), 1);
        QCOMPARE(tokenSpy.count(), 1);
    }

private:
    std::unique_ptr<QTemporaryDir> m_tmp;
    QString m_configPath;
    KSharedConfigPtr m_config;
};

QTEST_GUILESS_MAIN(TstMediaFusionSettings)
#include "tst_mediafusion_settings.moc"
