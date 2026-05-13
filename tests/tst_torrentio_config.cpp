// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/util/TorrentioConfig.h"

#include <QTest>

using namespace kinema::core::torrentio;

class TstTorrentioConfig : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    void defaults_isSortSeedersOnly()
    {
        QCOMPARE(toPathSegment({}), QStringLiteral("sort=seeders"));
    }

    void sortModes_stringifyCorrectly()
    {
        ConfigOptions o;
        o.sort = SortMode::Size;
        QCOMPARE(toPathSegment(o), QStringLiteral("sort=size"));
        o.sort = SortMode::QualitySize;
        QCOMPARE(toPathSegment(o), QStringLiteral("sort=qualitysize"));
    }

    void exclusionsAndProviders_joinedWithCommas()
    {
        ConfigOptions o;
        o.excludedResolutions = { QStringLiteral("4k"), QStringLiteral("480p") };
        o.excludedCategories = { QStringLiteral("cam"), QStringLiteral("threed") };
        o.providers = { QStringLiteral("yts"), QStringLiteral("eztv") };
        QCOMPARE(toPathSegment(o),
            QStringLiteral("sort=seeders|qualityfilter=4k,480p,cam,threed|providers=yts,eztv"));
    }

    void exclusions_resolutionsBeforeCategories()
    {
        ConfigOptions o;
        o.excludedResolutions = { QStringLiteral("1080p") };
        o.excludedCategories = { QStringLiteral("hdr") };
        QCOMPARE(toPathSegment(o),
            QStringLiteral("sort=seeders|qualityfilter=1080p,hdr"));
    }

    void exclusions_emptyListsOmitSegment()
    {
        ConfigOptions o;
        o.excludedResolutions = {};
        o.excludedCategories = {};
        o.providers = {};
        QCOMPARE(toPathSegment(o), QStringLiteral("sort=seeders"));
    }

    void exclusions_onlyCategories_renders()
    {
        ConfigOptions o;
        o.excludedCategories = { QStringLiteral("cam"), QStringLiteral("scr") };
        QCOMPARE(toPathSegment(o),
            QStringLiteral("sort=seeders|qualityfilter=cam,scr"));
    }

    void exclusions_onlyResolutions_renders()
    {
        ConfigOptions o;
        o.excludedResolutions = { QStringLiteral("480p") };
        QCOMPARE(toPathSegment(o),
            QStringLiteral("sort=seeders|qualityfilter=480p"));
    }

    void doesNotEmbedDebridUnlessConfigured()
    {
        // No `debridProvider`/`debridToken` → no debrid pair in URL,
        // even when other segments are populated.
        ConfigOptions o;
        o.excludedResolutions = { QStringLiteral("4k") };
        o.providers = { QStringLiteral("yts") };
        const auto s = toPathSegment(o);
        QVERIFY(!s.contains(QStringLiteral("realdebrid")));
        QVERIFY(!s.contains(QStringLiteral("alldebrid")));
    }

    void debrid_bothFieldsSet_appendsLastSegment()
    {
        ConfigOptions o;
        o.debridProvider = QStringLiteral("realdebrid");
        o.debridToken = QStringLiteral("rd-token");
        QCOMPARE(toPathSegment(o),
            QStringLiteral(
                "sort=seeders|realdebrid=rd-token"));
    }

    void debrid_alldebrid_appendsLastSegment()
    {
        ConfigOptions o;
        o.debridProvider = QStringLiteral("alldebrid");
        o.debridToken = QStringLiteral("ad-key");
        QCOMPARE(toPathSegment(o),
            QStringLiteral("sort=seeders|alldebrid=ad-key"));
    }

    void debrid_segmentOrderIsAfterQualityFilterAndProviders()
    {
        ConfigOptions o;
        o.excludedResolutions = { QStringLiteral("4k") };
        o.providers = { QStringLiteral("yts") };
        o.debridProvider = QStringLiteral("realdebrid");
        o.debridToken = QStringLiteral("rd-token");
        QCOMPARE(toPathSegment(o),
            QStringLiteral(
                "sort=seeders|qualityfilter=4k|providers=yts"
                "|realdebrid=rd-token"));
    }

    void debrid_orphanProviderIsDropped()
    {
        // A provider without a token must not emit `realdebrid=`
        // (would produce a malformed URL).
        ConfigOptions o;
        o.debridProvider = QStringLiteral("realdebrid");
        QCOMPARE(toPathSegment(o), QStringLiteral("sort=seeders"));
    }

    void debrid_orphanTokenIsDropped()
    {
        // A bare token with no provider is silently dropped.
        ConfigOptions o;
        o.debridToken = QStringLiteral("rd-token");
        QCOMPARE(toPathSegment(o), QStringLiteral("sort=seeders"));
    }
};

QTEST_APPLESS_MAIN(TstTorrentioConfig)
#include "tst_torrentio_config.moc"
