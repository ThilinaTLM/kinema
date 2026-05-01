// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Library.h"
#include "api/Media.h"

#include <QList>
#include <QObject>
#include <QString>

#include <optional>

namespace kinema::api {
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
 */
class LibraryController : public QObject
{
    Q_OBJECT
public:
    explicit LibraryController(core::LibraryStore& store,
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

Q_SIGNALS:
    void changed();
    void statusMessage(const QString& text, int timeoutMs = 3000);

private:
    core::LibraryStore& m_store;
};

} // namespace kinema::controllers
