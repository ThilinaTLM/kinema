// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include "api/Types.h"
#include "core/TorrentioConfig.h"

#include <QMainWindow>

#include <QCoro/QCoroTask>

#include <memory>
#include <optional>

class KActionCollection;
class KHamburgerMenu;
class QListView;
class QSplitter;
class QStackedWidget;
class QToolBar;

namespace kinema::core {
class HttpClient;
class PlayerLauncher;
class TokenStore;
}

namespace kinema::api {
class CinemetaClient;
class TmdbClient;
class TorrentioClient;
}

namespace kinema::ui {

class DetailPane;
class DiscoverPage;
class ImageLoader;
class ResultCardDelegate;
class ResultsModel;
class SearchBar;
class SeriesDetailPane;
class StateWidget;

namespace settings {
class SettingsDialog;
}

/**
 * Top-level window. Owns the HTTP stack, API clients, image loader, and
 * wires up the search → results → detail → torrents flow as coroutines.
 */
class MainWindow : public QMainWindow
{
    Q_OBJECT
public:
    explicit MainWindow(QWidget* parent = nullptr);
    ~MainWindow() override;

private Q_SLOTS:
    void onQueryRequested(const QString& text, api::MediaKind kind);
    void onResultActivated(const QModelIndex& index);
    void onEpisodeSelected(const api::Episode& ep);
    void onDiscoverItemActivated(const api::DiscoverItem& item);
    /// Toolbar / menu "Home" action — swaps the results stack back to
    /// the Discover page and closes any open detail panel.
    void showDiscoverHome();
    void onCopyMagnet(const api::Stream& stream);
    void onOpenMagnet(const api::Stream& stream);
    void onCopyDirectUrl(const api::Stream& stream);
    void onOpenDirectUrl(const api::Stream& stream);
    void onPlayRequested(const api::Stream& stream);
    void showAbout();
    void showSettings();
    void onTorrentioOptionsChanged();
    void onShowMenubarToggled(bool visible);

private:
    QCoro::Task<void> runSearch(QString text, api::MediaKind kind);
    QCoro::Task<void> loadMovieDetail(api::MetaSummary summary);
    QCoro::Task<void> loadSeriesDetail(api::MetaSummary summary);
    QCoro::Task<void> loadEpisodeStreams(api::Episode episode, QString imdbId);
    QCoro::Task<void> loadRealDebridToken();
    QCoro::Task<void> loadTmdbToken();
    QCoro::Task<void> openFromDiscover(api::DiscoverItem item);

    void buildActions();
    void buildLayout();
    void openDetailPanel(int stackIndex);
    void closeDetailPanel();
    core::torrentio::ConfigOptions currentConfig() const;

    // Ownership (parented to this window)
    std::unique_ptr<core::HttpClient> m_http;
    std::unique_ptr<core::TokenStore> m_tokens;
    std::unique_ptr<core::PlayerLauncher> m_player;
    api::CinemetaClient* m_cinemeta {};
    api::TorrentioClient* m_torrentio {};
    api::TmdbClient* m_tmdb {};
    ImageLoader* m_imageLoader {};

    // UI
    QToolBar* m_toolbar {};
    KActionCollection* m_actions {};
    QAction* m_showMenubarAction {};
    SearchBar* m_searchBar {};
    ResultsModel* m_resultsModel {};
    ResultCardDelegate* m_resultsDelegate {};
    QListView* m_resultsView {};
    StateWidget* m_resultsState {};
    QStackedWidget* m_resultsStack {};
    DiscoverPage* m_discoverPage {};
    DetailPane* m_detailPane {};
    SeriesDetailPane* m_seriesDetailPane {};

    // Outer horizontal splitter: grid (left) + detail stack (right).
    // The detail stack holds DetailPane (index 0) and SeriesDetailPane
    // (index 1); it is hidden when no result is selected so the grid
    // takes the full window width.
    QSplitter* m_browseSplitter {};
    QStackedWidget* m_detailStack {};
    // Last known splitter state with the panel open — restored when
    // the user re-opens the panel after a close.
    QByteArray m_savedSplitterOpenState;

    // Concurrency guard — increments on each new query so stale coroutines
    // can detect they've been superseded and bail out.
    quint64 m_queryEpoch = 0;
    quint64 m_detailEpoch = 0;
    quint64 m_episodeEpoch = 0;

    // Cached series context so we can re-fetch streams when the user
    // picks a different episode without re-fetching Cinemeta meta.
    QString m_currentSeriesImdbId;

    // Cached "what is currently on screen" pointers used by the
    // Settings-triggered refetch logic. Cleared on navigation away.
    std::optional<api::MetaSummary> m_currentMovie;
    std::optional<api::Episode> m_currentEpisode;

    // In-memory RD token, loaded at startup and whenever Settings saves.
    QString m_rdToken;

    // In-memory TMDB token. Resolved from: user keyring override →
    // compile-time default → empty. Re-resolved when Settings saves
    // or removes the user override.
    QString m_tmdbToken;

    // Settings dialog is created lazily and reused across invocations.
    settings::SettingsDialog* m_settingsDialog {};

    // Debounce flag for Config::torrentioOptionsChanged: Settings often
    // applies several sub-settings in sequence, emitting the signal
    // multiple times. We coalesce into a single refetch via a
    // zero-delay single-shot.
    bool m_refetchPending = false;
};

} // namespace kinema::ui
