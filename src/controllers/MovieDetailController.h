// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Discover.h"
#include "api/Media.h"

#include <QObject>
#include <QString>

#include <QCoro/QCoroTask>

#include <optional>

namespace kinema::api {
class CinemetaClient;
class TmdbClient;
class TorrentioClient;
}

namespace kinema::config {
class AppSettings;
}

namespace kinema::ui {
class DetailPane;
}

namespace kinema::controllers {

class NavigationController;

/**
 * Drives the movie-detail surface: Cinemeta meta + Torrentio streams.
 * Owns its own epoch so rapid navigation doesn't render stale results.
 *
 * Receives the current Real-Debrid token by reference (owned by
 * TokenController, which outlives this controller).
 */
class MovieDetailController : public QObject
{
    Q_OBJECT
public:
    MovieDetailController(
        api::CinemetaClient* cinemeta,
        api::TorrentioClient* torrentio,
        api::TmdbClient* tmdb,
        ui::DetailPane* pane,
        NavigationController* nav,
        const config::AppSettings& settings,
        const QString& rdTokenRef,
        QObject* parent = nullptr);

    /// The most-recently opened summary (or nullopt if none).
    const std::optional<api::MetaSummary>& current() const noexcept
    {
        return m_current;
    }

public Q_SLOTS:
    /// Entry from the search/results grid.
    void openFromSummary(const api::MetaSummary& summary);

    /// Entry from a Discover card (resolves imdb via TMDB then hands
    /// off to openFromSummary).
    void openFromDiscover(const api::DiscoverItem& item);

    /// Re-run the current movie's fetch (used by the
    /// Torrentio-options-changed debounce).
    void refetchCurrent();

    /// Clear all cached state. Called by MainWindow when the detail
    /// pane is closed / reset.
    void clear();

Q_SIGNALS:
    /// Status-bar message; MainWindow forwards to statusBar().
    void statusMessage(const QString& text, int timeoutMs = 3000);

private:
    QCoro::Task<void> loadTask(api::MetaSummary summary);
    QCoro::Task<void> openFromDiscoverTask(api::DiscoverItem item);

    api::CinemetaClient* m_cinemeta;
    api::TorrentioClient* m_torrentio;
    api::TmdbClient* m_tmdb;
    ui::DetailPane* m_pane;
    NavigationController* m_nav;
    const config::AppSettings& m_settings;
    const QString& m_rdToken;

    quint64 m_epoch = 0;
    std::optional<api::MetaSummary> m_current;
};

} // namespace kinema::controllers
