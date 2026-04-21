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
class KStatusNotifierItem;
class QListView;
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
    /// Toolbar / menu "Browse" action — swaps the results stack to
    /// the filter-bar Browse page and closes any open detail panel.
    void showBrowsePage();
    void onCopyMagnet(const api::Stream& stream);
    void onOpenMagnet(const api::Stream& stream);
    void onCopyDirectUrl(const api::Stream& stream);
    void onOpenDirectUrl(const api::Stream& stream);
    void onPlayRequested(const api::Stream& stream);
    void showAbout();
    void showSettings();
    void onTorrentioOptionsChanged();
    void onShowMenubarToggled(bool visible);
    /// Toolbar / menu "Back" action — walks one level up through the
    /// nav chain: streams page → episodes, detail → results, search
    /// results → Discover home.
    void onBackRequested();

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
    void showMovieDetail();
    void showSeriesDetail();
    void closeDetailPanel();
    void onBackToEpisodes();

#ifdef KINEMA_HAVE_LIBMPV
    /// Lazily create the detached player window, point mpv at `url`,
    /// and show/raise it. Re-used for subsequent Plays.
    void openEmbeddedPlayer(const QUrl& url, const QString& title);
#endif

    // ---- Tray + app lifecycle ---------------------------------------------
    /// Create the KStatusNotifierItem tray icon, context menu, and
    /// primary-activation handler. No-op when no tray host is
    /// available (GNOME without extensions, minimal WMs, …); in
    /// that case m_trayAvailable stays false and close-to-tray
    /// becomes inert.
    void buildTray();
    /// Refresh the tray context menu: toggle label / icon on the
    /// Show-Hide entry, and visibility of the Show-Player entry.
    void updateTrayMenu();
    /// Bring the main window to the foreground (raise + activate).
    void showMainWindow();
    /// Hide the main window without quitting. The tray icon remains
    /// so the user can get it back.
    void hideMainWindow();
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
    QAction* m_backAction {};
    QAction* m_browseAction {};
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

    // Central stack: page 0 = results (Discover / state / grid);
    // page 1 = movie detail; page 2 = series detail. Clicking a result
    // or Discover card swaps pages; closing / Esc / Home swaps back.
    QStackedWidget* m_centerStack {};

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

    // Detached embedded-mpv player window. Top-level QWidget that
    // hosts the MpvWidget so playback happens in its own window
    // alongside Kinema. Created lazily on first Play with
    // Kind::Embedded selected; reused for subsequent plays. Null
    // when the build was configured without libmpv.
    player::PlayerWindow* m_playerWindow {};

    // ---- Tray + quit ------------------------------------------------------
    KStatusNotifierItem* m_tray {};
    QAction* m_trayToggleAction {};
    QAction* m_trayShowPlayerAction {};
    bool m_trayAvailable {false};
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
