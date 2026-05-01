// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "TestDoubles.h"

#include "api/Library.h"
#include "controllers/LibraryController.h"
#include "core/Database.h"
#include "core/HttpError.h"
#include "core/LibraryStore.h"

#include <QSignalSpy>
#include <QTest>

using namespace kinema;

namespace {

api::LibraryTitle plainMovie(const QString& imdbId, const QString& title)
{
    api::LibraryTitle t;
    t.kind = api::MediaKind::Movie;
    t.imdbId = imdbId;
    t.title = title;
    t.year = 2024;
    return t;
}

api::LibraryTitle plainSeries(const QString& imdbId, const QString& title)
{
    api::LibraryTitle t;
    t.kind = api::MediaKind::Series;
    t.imdbId = imdbId;
    t.title = title;
    t.year = 2024;
    return t;
}

api::MetaDetail richMovieMeta(const QString& imdbId)
{
    api::MetaDetail m;
    m.summary.kind = api::MediaKind::Movie;
    m.summary.imdbId = imdbId;
    m.summary.title = QStringLiteral("Backfilled Movie");
    m.summary.year = 2024;
    m.summary.imdbRating = 7.7;
    m.genres = { QStringLiteral("Drama"), QStringLiteral("Sci-Fi") };
    m.runtimeMinutes = 121;
    m.cast = { QStringLiteral("Actor A"), QStringLiteral("Actor B") };
    return m;
}

api::SeriesDetail richSeriesMeta(const QString& imdbId)
{
    api::SeriesDetail sd;
    sd.meta.summary.kind = api::MediaKind::Series;
    sd.meta.summary.imdbId = imdbId;
    sd.meta.summary.title = QStringLiteral("Backfilled Series");
    sd.meta.summary.year = 2023;
    sd.meta.summary.imdbRating = 8.3;
    sd.meta.genres = { QStringLiteral("Crime") };
    sd.meta.runtimeMinutes = 45;
    sd.meta.cast = { QStringLiteral("Lead") };
    return sd;
}

} // namespace

class TstLibraryBackfill : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void init()
    {
        m_db = std::make_unique<core::Database>(
            QStringLiteral(":memory:"), nullptr);
        QVERIFY(m_db->open());
        m_store = std::make_unique<core::LibraryStore>(*m_db);
        m_cinemeta = std::make_unique<tests::FakeCinemetaClient>();
        m_ctrl = std::make_unique<controllers::LibraryController>(
            *m_store, m_cinemeta.get());
    }

    void cleanup()
    {
        m_ctrl.reset();
        m_cinemeta.reset();
        m_store.reset();
        m_db.reset();
    }

    void backfillsMovieFromCinemeta()
    {
        m_store->upsertTitle(plainMovie(
            QStringLiteral("tt7000001"),
            QStringLiteral("Pre-v7 Movie")));

        tests::FakeCinemetaClient::MetaScript scripted;
        scripted.detail = richMovieMeta(QStringLiteral("tt7000001"));
        m_cinemeta->metaScripts.append(scripted);

        m_ctrl->backfillMetadata();
        tests::drainEvents(8);

        const auto got = m_store->find(api::MediaKind::Movie,
            QStringLiteral("tt7000001"));
        QVERIFY(got.has_value());
        QCOMPARE(got->genres, QStringList({ QStringLiteral("Drama"),
            QStringLiteral("Sci-Fi") }));
        QVERIFY(got->imdbRating.has_value());
        QCOMPARE(*got->imdbRating, 7.7);
        QCOMPARE(got->runtimeMinutes.value_or(0), 121);
        QCOMPARE(got->cast, QStringList({ QStringLiteral("Actor A"),
            QStringLiteral("Actor B") }));
        // Backfill should not change addedAt.
        const auto stored = m_store->find(api::MediaKind::Movie,
            QStringLiteral("tt7000001"));
        QVERIFY(stored.has_value());
    }

    void backfillsSeriesFromCinemeta()
    {
        m_store->upsertTitle(plainSeries(
            QStringLiteral("tt7000002"),
            QStringLiteral("Pre-v7 Series")));

        tests::FakeCinemetaClient::SeriesScript scripted;
        scripted.detail = richSeriesMeta(QStringLiteral("tt7000002"));
        m_cinemeta->seriesScripts.append(scripted);

        m_ctrl->backfillMetadata();
        tests::drainEvents(8);

        const auto got = m_store->find(api::MediaKind::Series,
            QStringLiteral("tt7000002"));
        QVERIFY(got.has_value());
        QCOMPARE(got->genres, QStringList({ QStringLiteral("Crime") }));
        QCOMPARE(got->runtimeMinutes.value_or(0), 45);
    }

    void skipsRowsWithExistingMetadata()
    {
        // Pre-populate a movie that already has v7 metadata. Backfill
        // must not call Cinemeta at all for it.
        api::LibraryTitle t = plainMovie(
            QStringLiteral("tt7000003"),
            QStringLiteral("Already Filled"));
        t.genres = { QStringLiteral("Drama") };
        t.imdbRating = 6.0;
        m_store->upsertTitle(t);

        m_ctrl->backfillMetadata();
        tests::drainEvents(4);

        QCOMPARE(m_cinemeta->metaCalls, 0);
        QCOMPARE(m_cinemeta->seriesCalls, 0);
    }

    void surfaceErrorIsSilentAndPreservesRow()
    {
        m_store->upsertTitle(plainMovie(
            QStringLiteral("tt7000004"),
            QStringLiteral("Will Fail")));

        tests::FakeCinemetaClient::MetaScript scripted;
        scripted.error = core::HttpError(core::HttpError::Kind::Network,
            0, QStringLiteral("offline"));
        m_cinemeta->metaScripts.append(scripted);

        m_ctrl->backfillMetadata();
        tests::drainEvents(4);

        const auto got = m_store->find(api::MediaKind::Movie,
            QStringLiteral("tt7000004"));
        QVERIFY(got.has_value());
        QVERIFY(got->genres.isEmpty()); // unchanged
        QVERIFY(!got->imdbRating.has_value());
    }

private:
    std::unique_ptr<core::Database> m_db;
    std::unique_ptr<core::LibraryStore> m_store;
    std::unique_ptr<tests::FakeCinemetaClient> m_cinemeta;
    std::unique_ptr<controllers::LibraryController> m_ctrl;
};

QTEST_MAIN(TstLibraryBackfill)
#include "tst_library_backfill.moc"
