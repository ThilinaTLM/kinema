// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Library.h"
#include "api/PlaybackContext.h"

#include <QHash>
#include <QList>
#include <QObject>
#include <QString>

#include <optional>

namespace kinema::core {

class Database;

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
    bool isActive(api::MediaKind kind, const QString& imdbId) const;

    QList<api::LibraryTitle> activeTitles() const;
    QList<api::LibraryTitle> allTitles(bool includeInactive = false) const;

    QList<api::LibraryEpisode> episodesForSeries(
        const QString& imdbId) const;
    std::optional<api::LibraryEpisode> episode(
        const QString& imdbId, int season, int episode) const;

    void upsertTitle(const api::LibraryTitle& title);
    void upsertEpisodes(const QString& seriesImdbId,
        const QList<api::LibraryEpisode>& episodes);
    void setActive(api::MediaKind kind, const QString& imdbId, bool active);
    void hardDelete(api::MediaKind kind, const QString& imdbId);

    api::LibraryWatchOverride watchOverride(
        const api::PlaybackKey& key) const;
    QHash<QString, api::LibraryWatchOverride> watchOverridesForImdb(
        const QString& imdbId) const;
    void setWatchOverride(const api::PlaybackKey& key,
        api::LibraryWatchOverride state);
    void clearWatchOverride(const api::PlaybackKey& key);

Q_SIGNALS:
    void changed();

private:
    void scheduleChanged();

    Database& m_db;
    bool m_changePending = false;
};

} // namespace kinema::core
