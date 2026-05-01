// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Library.h"
#include "api/Media.h"

#include <QCoro/QCoroTask>

#include <QList>
#include <QObject>
#include <QQueue>
#include <QString>

#include <optional>

namespace kinema::api {
class CinemetaClient;
struct MetaDetail;
struct SeriesDetail;
}

namespace kinema::core {
class LibraryStore;
}

namespace kinema::controllers {

/**
 * Library actions: save / remove curated titles. Watched-state and
 * playback-progress queries live in `controllers::WatchedController`
 * and `controllers::HistoryController` respectively \u2014 this
 * controller is intentionally narrow.
 *
 * The optional `CinemetaClient*` powers `backfillMetadata()`, used
 * to fill in the schema-v7 columns (`genres`, `imdb_rating`,
 * `runtime_minutes`, `cast_list`) for rows saved before that
 * migration. Tests that don't need backfill can pass `nullptr`.
 */
class LibraryController : public QObject
{
    Q_OBJECT
public:
    explicit LibraryController(core::LibraryStore& store,
        api::CinemetaClient* cinemeta = nullptr,
        QObject* parent = nullptr);
    ~LibraryController() override;

    bool isInLibrary(api::MediaKind kind, const QString& imdbId) const;
    std::optional<api::LibraryTitle> title(api::MediaKind kind,
        const QString& imdbId) const;
    QList<api::LibraryTitle> titles() const;
    QList<api::LibraryEpisode> episodesForSeries(
        const QString& imdbId) const;

    void saveMovie(const api::MetaDetail& meta);
    void saveSeries(const api::SeriesDetail& detail);
    /// Remove the title (and, for series, its cached episode rows)
    /// from the library. Watched-state and history rows are
    /// preserved.
    void removeFromLibrary(api::MediaKind kind, const QString& imdbId);

public Q_SLOTS:
    /// Walk the saved titles and ask Cinemeta for any whose v7
    /// metadata columns are still empty. Idempotent and silent on
    /// failure; safe to call from a queued connection at startup.
    /// Capped at a small number of in-flight requests so a large
    /// library doesn't flood Cinemeta.
    void backfillMetadata();

Q_SIGNALS:
    void changed();
    void statusMessage(const QString& text, int timeoutMs = 3000);

private:
    void pumpBackfill();
    QCoro::Task<void> runBackfillOne(api::LibraryTitle seed);

    core::LibraryStore& m_store;
    api::CinemetaClient* m_cinemeta {};
    QQueue<api::LibraryTitle> m_backfillQueue;
    int m_backfillInFlight = 0;
};

} // namespace kinema::controllers
