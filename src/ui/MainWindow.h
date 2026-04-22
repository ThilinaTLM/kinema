// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Discover.h"
#include "api/Media.h"
#include "api/PlaybackContext.h"

#include <QMainWindow>

#include <QCoro/QCoroTask>

#include <memory>
#include <optional>

class KActionCollection;
class KHamburgerMenu;
class QListView;
class QStackedWidget;
class QToolBar;

namespace kinema::core {
class Database;
class HistoryStore;
class HttpClient;
class PlayerLauncher;
class TokenStore;
}

namespace kinema::api {
class CinemetaClient;
class TmdbClient;
class TorrentioClient;
}

namespace kinema::config {
class AppSettings;
}

namespace kinema::controllers {
class HistoryController;
class MovieDetailController;
class NavigationController;
class SearchController;
class SeriesDetailController;
class TokenController;
class TrayController;
}

namespace kinema::services {
class StreamActions;
}

namespace kinema::ui {

namespace player {
class MpvWidget;
class PlayerWindow;
}

class BrowsePage;
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
    explicit MainWindow(config::AppSettings& settings,
        QWidget* parent = nullptr);
    ~MainWindow() override;

private Q_SLOTS:
    void onQueryRequested(const QString& text, api::MediaKind kind);
    void onResultActivated(const QModelIndex& index);
    void onEpisodeSelected(const api::Episode& ep);
    void onDiscoverItemActivated(const api::DiscoverItem& item);
    /// Toolbar / menu "Home" action — swaps the results stack back to
    /// the Discover page and closes any open detail panel.
    void showDiscoverHome();
    /// Toolbar / menu "Browse" action — swaps the results stack to
    /// the filter-bar Browse page and closes any open detail panel.
    void showBrowsePage();

    void showAbout();
    void showSettings();
    void onTorrentioOptionsChanged();
    void onShowMenubarToggled(bool visible);
    /// Toolbar / menu "Back" action — walks one level up through the
    /// nav chain: streams page → episodes, detail → results, search
    /// results → Discover home.
    void onBackRequested();

    /// Tools → Clear watch history… — drops every row from the
    /// history DB after a confirmation dialog.
    void onClearHistoryRequested();

private:
    void buildCoreServices();
    void buildActions();
    void buildLayout();
    void buildSearchSurface();
    void buildResultsPages();
    void buildDetailPages();
    void buildControllers();
    void wireStatusSignals();
    void wireRealDebridState();
    void wireTray();
    void buildMenuBar(QAction* quitAction,
        QAction* focusSearch,
        QAction* homeAction,
        QAction* prefsAction,
        QAction* aboutAction);
    KHamburgerMenu* buildHamburgerMenu(QAction* quitAction,
        QAction* focusSearch,
        QAction* homeAction,
        QAction* prefsAction,
        QAction* aboutAction);
    void buildToolbarActions(QAction* homeAction, KHamburgerMenu* hamburger);
    void restoreMenubarVisibility();
    void buildEscapeShortcut();
    void showMovieDetail();
    void showSeriesDetail();
    void closeDetailPanel();
    void onBackToEpisodes();

#ifdef KINEMA_HAVE_LIBMPV
    /// Lazily create the detached player window, point mpv at `url`,
    /// and show/raise it. Re-used for subsequent Plays. `ctx`
    /// carries display title + optional resume-from timestamp.
    void openEmbeddedPlayer(const QUrl& url,
        const api::PlaybackContext& ctx);
#endif

    // ---- Window lifecycle -------------------------------------------------
    /// Bring the main window to the foreground (raise + activate).
    void showMainWindow();
    /// Hide the main window without quitting. The tray icon remains
    /// so the user can get it back.
    void hideMainWindow();
    /// Toggle visibility; called from TrayController.
    void toggleMainWindow();
    /// Called from every "real quit" path (tray Quit, KStandardAction::quit,
    /// Ctrl+Q). Sets m_reallyQuit so closeEvent stops hiding and
    /// then calls qApp->quit().
    void quitApplication();

    // QWidget / QMainWindow override — routes close to hide-to-tray
    // when the preference is on and a tray is available.
    void closeEvent(QCloseEvent* event) override;
    /// Refresh the toolbar Back action's enabled state so it matches
    /// the current navigation position (disabled only on Discover
    /// home with no detail pane open).
    void updateBackActionEnabled();
    // Settings are owned by KinemaApplication and outlive this window.
    config::AppSettings& m_settings;

    // Ownership (parented to this window)
    std::unique_ptr<core::HttpClient> m_http;
    std::unique_ptr<core::TokenStore> m_tokens;
    std::unique_ptr<core::PlayerLauncher> m_player;
    std::unique_ptr<core::Database> m_db;
    std::unique_ptr<core::HistoryStore> m_history;
    api::CinemetaClient* m_cinemeta {};
    api::TorrentioClient* m_torrentio {};
    api::TmdbClient* m_tmdb {};
    ImageLoader* m_imageLoader {};

    // UI
    QToolBar* m_toolbar {};
    KActionCollection* m_actions {};
    QAction* m_showMenubarAction {};
    QAction* m_backAction {};
    QAction* m_browseAction {};
    QAction* m_clearHistoryAction {};
    SearchBar* m_searchBar {};
    ResultsModel* m_resultsModel {};
    ResultCardDelegate* m_resultsDelegate {};
    QListView* m_resultsView {};
    StateWidget* m_resultsState {};
    QStackedWidget* m_resultsStack {};
    DiscoverPage* m_discoverPage {};
    BrowsePage* m_browsePage {};
    DetailPane* m_detailPane {};
    SeriesDetailPane* m_seriesDetailPane {};

    controllers::NavigationController* m_nav {};
    controllers::SearchController* m_search {};
    controllers::MovieDetailController* m_movieCtrl {};
    controllers::SeriesDetailController* m_seriesCtrl {};
    controllers::TokenController* m_tokenCtrl {};
    controllers::TrayController* m_tray {};
    controllers::HistoryController* m_historyCtrl {};
    services::StreamActions* m_streamActions {};

    // Central stack: page 0 = results (Discover / state / grid);
    // page 1 = movie detail; page 2 = series detail. Clicking a result
    // or Discover card swaps pages; closing / Esc / Home swaps back.
    QStackedWidget* m_centerStack {};
    // Settings dialog is created lazily and reused across invocations.
    settings::SettingsDialog* m_settingsDialog {};

    // Detached embedded-mpv player window. Top-level QWidget that
    // hosts the MpvWidget so playback happens in its own window
    // alongside Kinema. Created lazily on first Play with
    // Kind::Embedded selected; reused for subsequent plays. Null
    // when the build was configured without libmpv.
    player::PlayerWindow* m_playerWindow {};

#ifdef KINEMA_HAVE_LIBMPV
    // Context of the most recent embedded-play handoff. Remembered
    // so an "error" endOfFile can offer "Choose another release…"
    // for the right media.
    std::optional<api::PlaybackContext> m_lastEmbeddedPlay;
#endif

    // Set by quitApplication() so closeEvent accepts the close
    // instead of hiding the main window.
    bool m_reallyQuit {false};
    // One-shot per-session flag so we only show the
    // "still running in the tray" toast the first time the user
    // closes the main window.
    bool m_hasShownTrayToast {false};

    // Debounce flag for Config::torrentioOptionsChanged: Settings often
    // applies several sub-settings in sequence, emitting the signal
    // multiple times. We coalesce into a single refetch via a
    // zero-delay single-shot.
    bool m_refetchPending = false;
};

} // namespace kinema::ui
