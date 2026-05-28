// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/StreamSorting.h"

#include <QTest>

using namespace kinema;
using namespace kinema::ui::qml::stream_sorting;
using SortMode = kinema::ui::qml::StreamsListModel::SortMode;

namespace {

domain::Stream makeStream(const QString& releaseName,
    const QString& resolution, int seeders, bool cached = false)
{
    domain::Stream s;
    s.releaseName = releaseName;
    s.resolution = resolution;
    s.seeders = seeders;
    s.debridCached = cached;
    return s;
}

} // namespace

class TstStreamSorting : public QObject
{
    Q_OBJECT
private Q_SLOTS:
    // ---- Smart sort: cached-first ---------------------------------
    void smart_cachedRowsLeadAtEqualResolution()
    {
        QList<domain::Stream> rows {
            makeStream(QStringLiteral("uncached-1080p"), QStringLiteral("1080p"),
                50, /*cached=*/false),
            makeStream(QStringLiteral("cached-1080p"), QStringLiteral("1080p"),
                10, /*cached=*/true),
        };
        sortInPlace(rows, SortMode::Smart, /*descending=*/true);
        // Cached wins even though it has fewer seeders.
        QCOMPARE(rows[0].releaseName, QStringLiteral("cached-1080p"));
        QCOMPARE(rows[1].releaseName, QStringLiteral("uncached-1080p"));
    }

    void smart_cachedDoesNotOverrideResolutionAcrossTiers()
    {
        // Cached is the primary key, so a cached 720p sorts above an
        // uncached 2160p. This is intentional: instant playback first.
        QList<domain::Stream> rows {
            makeStream(QStringLiteral("uncached-2160p"), QStringLiteral("2160p"),
                100, /*cached=*/false),
            makeStream(QStringLiteral("cached-720p"), QStringLiteral("720p"),
                5, /*cached=*/true),
        };
        sortInPlace(rows, SortMode::Smart, true);
        QCOMPARE(rows[0].releaseName, QStringLiteral("cached-720p"));
        QCOMPARE(rows[1].releaseName, QStringLiteral("uncached-2160p"));
    }

    void smart_resolutionThenSeedersWithinCachedGroup()
    {
        QList<domain::Stream> rows {
            makeStream(QStringLiteral("c-1080p-lo"), QStringLiteral("1080p"),
                10, true),
            makeStream(QStringLiteral("c-2160p"), QStringLiteral("2160p"),
                1, true),
            makeStream(QStringLiteral("c-1080p-hi"), QStringLiteral("1080p"),
                90, true),
        };
        sortInPlace(rows, SortMode::Smart, true);
        QCOMPARE(rows[0].releaseName, QStringLiteral("c-2160p"));
        QCOMPARE(rows[1].releaseName, QStringLiteral("c-1080p-hi"));
        QCOMPARE(rows[2].releaseName, QStringLiteral("c-1080p-lo"));
    }

    void smart_noDebridIsResolutionThenSeeders()
    {
        // With no cached rows the cached key is inert and ordering
        // collapses to the historical resolution -> seeders shape.
        QList<domain::Stream> rows {
            makeStream(QStringLiteral("720p"), QStringLiteral("720p"), 99),
            makeStream(QStringLiteral("1080p-lo"), QStringLiteral("1080p"), 5),
            makeStream(QStringLiteral("1080p-hi"), QStringLiteral("1080p"), 80),
        };
        sortInPlace(rows, SortMode::Smart, true);
        QCOMPARE(rows[0].releaseName, QStringLiteral("1080p-hi"));
        QCOMPARE(rows[1].releaseName, QStringLiteral("1080p-lo"));
        QCOMPARE(rows[2].releaseName, QStringLiteral("720p"));
    }

    // ---- Cached-only UI filter ------------------------------------
    void cachedOnly_keepsOnlyCachedRows()
    {
        QList<domain::Stream> rows {
            makeStream(QStringLiteral("a"), QStringLiteral("1080p"), 10, true),
            makeStream(QStringLiteral("b"), QStringLiteral("1080p"), 10, false),
            makeStream(QStringLiteral("c"), QStringLiteral("720p"), 10, true),
        };
        UiFilters f;
        f.cachedOnly = true;
        const auto out = applyUiFilters(rows, f);
        QCOMPARE(out.size(), 2);
        QCOMPARE(out[0].releaseName, QStringLiteral("a"));
        QCOMPARE(out[1].releaseName, QStringLiteral("c"));
    }

    void cachedOnly_inactiveByDefaultPassesEverything()
    {
        QList<domain::Stream> rows {
            makeStream(QStringLiteral("a"), QStringLiteral("1080p"), 10, true),
            makeStream(QStringLiteral("b"), QStringLiteral("1080p"), 10, false),
        };
        const auto out = applyUiFilters(rows, {});
        QCOMPARE(out.size(), 2);
    }

    void uiFilters_anyReportsCachedOnly()
    {
        UiFilters f;
        QVERIFY(!f.any());
        f.cachedOnly = true;
        QVERIFY(f.any());
    }
};

QTEST_GUILESS_MAIN(TstStreamSorting)
#include "tst_stream_sorting.moc"
