// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/TorrentioConfig.h"

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

    void realDebrid_isAlwaysLast()
    {
        ConfigOptions o;
        o.excludedResolutions = { QStringLiteral("4k") };
        o.excludedCategories = { QStringLiteral("cam") };
        o.providers = { QStringLiteral("yts") };
        o.realDebridToken = QStringLiteral("SECRETTOKEN");
        const auto s = toPathSegment(o);

        QVERIFY(s.endsWith(QStringLiteral("|realdebrid=SECRETTOKEN")));
        QVERIFY(s.indexOf(QStringLiteral("qualityfilter"))
            < s.indexOf(QStringLiteral("realdebrid")));
        QVERIFY(s.indexOf(QStringLiteral("providers"))
            < s.indexOf(QStringLiteral("realdebrid")));
    }

    void realDebrid_onlyOption_stillLast()
    {
        ConfigOptions o;
        o.realDebridToken = QStringLiteral("T");
        QCOMPARE(toPathSegment(o), QStringLiteral("sort=seeders|realdebrid=T"));
    }
};

QTEST_APPLESS_MAIN(TstTorrentioConfig)
#include "tst_torrentio_config.moc"
