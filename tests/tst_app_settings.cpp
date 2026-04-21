// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/AppSettings.h"

#include <KConfig>
#include <KSharedConfig>

#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>

using namespace kinema;

class TstAppSettings : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void init()
    {
        m_tmpdir = std::make_unique<QTemporaryDir>();
        QVERIFY(m_tmpdir->isValid());
        // Give each test an isolated config file so we don't fight
        // over the user's real kinemarc.
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

    void testDefaultsWhenConfigEmpty()
    {
        config::AppSettings s(m_config);
        QCOMPARE(static_cast<int>(s.search().kind()),
            static_cast<int>(api::MediaKind::Movie));
        QCOMPARE(static_cast<int>(s.torrentio().defaultSort()),
            static_cast<int>(core::torrentio::SortMode::Seeders));
        QCOMPARE(s.torrentio().cachedOnly(), false);
        QCOMPARE(s.appearance().closeToTray(), true);
        QCOMPARE(s.appearance().showMenuBar(), false);
        QCOMPARE(s.realDebrid().configured(), false);
        QVERIFY(s.filter().excludedResolutions().isEmpty());
        QVERIFY(s.filter().excludedCategories().isEmpty());
        QVERIFY(s.filter().keywordBlocklist().isEmpty());
        QCOMPARE(static_cast<int>(s.browse().kind()),
            static_cast<int>(api::MediaKind::Movie));
        QCOMPARE(s.browse().minRatingPct(), 0);
        QCOMPARE(static_cast<int>(s.browse().sort()),
            static_cast<int>(api::DiscoverSort::Popularity));
        QCOMPARE(s.browse().hideObscure(), true);
    }

    // ---- Roundtrip -------------------------------------------------------

    void testSearchKindRoundtrip()
    {
        config::AppSettings s(m_config);
        s.search().setKind(api::MediaKind::Series);

        config::AppSettings s2(m_config);
        QCOMPARE(static_cast<int>(s2.search().kind()),
            static_cast<int>(api::MediaKind::Series));
    }

    void testFilterSettingsRoundtrip()
    {
        config::AppSettings s(m_config);
        s.filter().setExcludedResolutions({ QStringLiteral("4k"),
            QStringLiteral("1080p") });
        s.filter().setKeywordBlocklist({ QStringLiteral("HINDI"),
            QStringLiteral("YIFY") });

        config::AppSettings s2(m_config);
        QCOMPARE(s2.filter().excludedResolutions(),
            (QStringList { QStringLiteral("4k"),
                QStringLiteral("1080p") }));
        QCOMPARE(s2.filter().keywordBlocklist(),
            (QStringList { QStringLiteral("HINDI"),
                QStringLiteral("YIFY") }));
    }

    void testBrowseSettingsRoundtrip()
    {
        config::AppSettings s(m_config);
        s.browse().setKind(api::MediaKind::Series);
        s.browse().setGenreIds({ 28, 12, 35 });
        s.browse().setMinRatingPct(70);
        s.browse().setSort(api::DiscoverSort::Rating);
        s.browse().setHideObscure(false);

        config::AppSettings s2(m_config);
        QCOMPARE(static_cast<int>(s2.browse().kind()),
            static_cast<int>(api::MediaKind::Series));
        QCOMPARE(s2.browse().genreIds(), (QList<int> { 28, 12, 35 }));
        QCOMPARE(s2.browse().minRatingPct(), 70);
        QCOMPARE(static_cast<int>(s2.browse().sort()),
            static_cast<int>(api::DiscoverSort::Rating));
        QCOMPARE(s2.browse().hideObscure(), false);
    }

    // ---- Signals ---------------------------------------------------------

    void testTorrentioOptionsChangedEmittedOnSortChange()
    {
        config::AppSettings s(m_config);
        QSignalSpy spy(&s, &config::AppSettings::torrentioOptionsChanged);
        s.torrentio().setDefaultSort(core::torrentio::SortMode::Size);
        QCOMPARE(spy.count(), 1);
    }

    void testTorrentioOptionsChangedEmittedOnExclusionsChange()
    {
        config::AppSettings s(m_config);
        QSignalSpy spy(&s, &config::AppSettings::torrentioOptionsChanged);
        s.filter().setExcludedResolutions({ QStringLiteral("4k") });
        QCOMPARE(spy.count(), 1);
        s.filter().setExcludedCategories({ QStringLiteral("cam") });
        QCOMPARE(spy.count(), 2);
    }

    void testCachedOnlyChangedEmitted()
    {
        config::AppSettings s(m_config);
        QSignalSpy spy(&s.torrentio(),
            &config::TorrentioSettings::cachedOnlyChanged);
        s.torrentio().setCachedOnly(true);
        QCOMPARE(spy.count(), 1);
        QCOMPARE(spy.takeFirst().at(0).toBool(), true);
    }

    void testRealDebridConfiguredSignal()
    {
        config::AppSettings s(m_config);
        QSignalSpy spy(&s.realDebrid(),
            &config::RealDebridSettings::configuredChanged);
        s.realDebrid().setConfigured(true);
        QCOMPARE(spy.count(), 1);
        // Idempotent set — no extra emission.
        s.realDebrid().setConfigured(true);
        QCOMPARE(spy.count(), 1);
    }

    void testKeywordBlocklistChangedSignal()
    {
        config::AppSettings s(m_config);
        QSignalSpy spy(&s.filter(),
            &config::FilterSettings::keywordBlocklistChanged);
        s.filter().setKeywordBlocklist({ QStringLiteral("abc") });
        QCOMPARE(spy.count(), 1);
    }

    // ---- Aggregated torrentioOptions() ----------------------------------

    void testTorrentioOptionsAggregates()
    {
        config::AppSettings s(m_config);
        s.torrentio().setDefaultSort(
            core::torrentio::SortMode::QualitySize);
        s.filter().setExcludedResolutions({ QStringLiteral("4k") });
        s.filter().setExcludedCategories({ QStringLiteral("cam") });

        const auto opts = s.torrentioOptions();
        QCOMPARE(static_cast<int>(opts.sort),
            static_cast<int>(core::torrentio::SortMode::QualitySize));
        QCOMPARE(opts.excludedResolutions,
            (QStringList { QStringLiteral("4k") }));
        QCOMPARE(opts.excludedCategories,
            (QStringList { QStringLiteral("cam") }));
        QVERIFY(opts.realDebridToken.isEmpty());
    }

private:
    std::unique_ptr<QTemporaryDir> m_tmpdir;
    QString m_configPath;
    KSharedConfigPtr m_config;
};

QTEST_MAIN(TstAppSettings)
#include "tst_app_settings.moc"
