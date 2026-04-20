// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

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

    void qualityAndProviders_joinedWithCommas()
    {
        ConfigOptions o;
        o.qualityFilter = { QStringLiteral("1080p"), QStringLiteral("2160p") };
        o.providers = { QStringLiteral("yts"), QStringLiteral("eztv") };
        QCOMPARE(toPathSegment(o),
            QStringLiteral("sort=seeders|qualityfilter=1080p,2160p|providers=yts,eztv"));
    }

    void emptyQualityAndProviders_areOmitted()
    {
        ConfigOptions o;
        o.qualityFilter = {};
        o.providers = {};
        QCOMPARE(toPathSegment(o), QStringLiteral("sort=seeders"));
    }

    void realDebrid_isAlwaysLast()
    {
        ConfigOptions o;
        o.qualityFilter = { QStringLiteral("1080p") };
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
