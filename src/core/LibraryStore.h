// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Library.h"

#include <QList>
#include <QObject>
#include <QString>

#include <optional>

namespace kinema::core {

class Database;

/**
 * DAO for the user's curated library: `library_titles` (movies and
 * series the user wants to keep around) and `library_episodes` (the
 * cached Cinemeta episode list for series in the library, used to
 * paint Library page rails without a live network call).
 *
 * The store does **not** know anything about watched-state or playback
 * progress. Those concerns live in `core::WatchedStore` /
 * `controllers::WatchedController` and `core::HistoryStore` /
 * `controllers::HistoryController`.
 *
 * Removal is hard delete only. Pre-1.0 carried a soft-delete (`active`
 * column) that was removed in migration v6.
 */
class LibraryStore : public QObject
{
    Q_OBJECT
public:
    explicit LibraryStore(Database& db, QObject* parent = nullptr);
    ~LibraryStore() override;

    LibraryStore(const LibraryStore&) = delete;
    LibraryStore& operator=(const LibraryStore&) = delete;

    std::optional<api::LibraryTitle> find(
        api::MediaKind kind, const QString& imdbId) const;
    bool contains(api::MediaKind kind, const QString& imdbId) const;

    /// All saved titles, most-recently-updated first.
    QList<api::LibraryTitle> titles() const;

    QList<api::LibraryEpisode> episodesForSeries(
        const QString& imdbId) const;
    std::optional<api::LibraryEpisode> episode(
        const QString& imdbId, int season, int episode) const;

    void upsertTitle(const api::LibraryTitle& title);
    void upsertEpisodes(const QString& seriesImdbId,
        const QList<api::LibraryEpisode>& episodes);
    /// Remove a title from the library. For series this also removes
    /// every cached episode row. Watched-state and history rows are
    /// preserved \u2014 they are not library data.
    void remove(api::MediaKind kind, const QString& imdbId);

Q_SIGNALS:
    void changed();

private:
    void scheduleChanged();

    Database& m_db;
    bool m_changePending = false;
};

} // namespace kinema::core
