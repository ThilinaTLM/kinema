// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <KAboutData>

#include <QObject>
#include <QString>

#include <memory>

class QQmlApplicationEngine;
class QQuickWindow;
class QUrl;

namespace kinema::api {
class CinemetaClient;
class OpenSubtitlesClient;
class TmdbClient;
class TorrentioClient;
class PlaybackContext;
}

namespace kinema::config {
class AppSettings;
}

namespace kinema::core {
class Database;
class HistoryStore;
class HttpClient;
class PlayerLauncher;
class SubtitleCacheStore;
class TokenStore;
}

namespace kinema::controllers {
class HistoryController;
class PlaybackController;
class SubtitleController;
class TokenController;
class TrayController;
}

namespace kinema::services {
class StreamActions;
}

namespace kinema::ui {
class ImageLoader;
namespace player {
class PlayerWindow;
}
}

namespace kinema::ui::qml {

class BrowseViewModel;
class ContinueWatchingViewModel;
class DiscoverViewModel;
class MovieDetailViewModel;
class SearchViewModel;
class SeriesDetailViewModel;

/**
 * Top-level QML host. Replaces `MainWindow`'s composition role:
 * owns every long-lived service the app needs (HTTP, DB, tokens,
 * history, subtitles, tray, embedded playback) and exposes itself
 * to QML as the `mainController` context property.
 *
 * Phase 02 surface covers the application shell: drawer actions,
 * keyboard-shortcut targets, tray ownership, the close-to-tray
 * decision tree, KAboutData feed for `Kirigami.AboutPage`, and a
 * passive-notification fan-in for status messages from the
 * controllers under it.
 *
 * View-models for individual pages (Discover, Search, Browse,
 * Detail, Subtitles, Settings) are registered alongside this
 * controller in phases 03–06.
 */
class MainController : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString applicationName READ applicationName CONSTANT)
    Q_PROPERTY(KAboutData aboutData READ aboutData CONSTANT)
    // Both flags are effectively constant for the application's
    // lifetime: tray availability is decided at TrayController
    // construction, and `closeToTray` doesn't yet emit a change
    // signal (KConfig watcher is wired in phase 06 alongside the
    // settings UI). They use READ-only properties for now.
    Q_PROPERTY(bool hideToTrayEnabled READ hideToTrayEnabled CONSTANT)
    Q_PROPERTY(bool trayAvailable READ trayAvailable CONSTANT)

public:
    explicit MainController(config::AppSettings& settings,
        QQmlApplicationEngine& engine, QObject* parent = nullptr);
    ~MainController() override;

    /// Called by `main.cpp` after `engine.load(...)` succeeds.
    /// Caches the root `QQuickWindow*` and uses it as transient
    /// parent for the embedded player and as the activation
    /// target for the tray.
    void attachWindow(QQuickWindow* window);

    QString applicationName() const;
    KAboutData aboutData() const;
    bool hideToTrayEnabled() const;
    bool trayAvailable() const;

    /// Decision struct returned by the testable close evaluator
    /// below; `handleWindowCloseRequested` collapses it into a
    /// single bool for QML's `onClosing` handler.
    struct CloseDecision {
        bool acceptClose;
        bool hideWindow;
        bool emitToast;
    };
    /// Pure function form of the close-event branching matrix.
    /// Lets `tst_main_controller_close_logic` exercise every path
    /// without building a full MainController.
    static CloseDecision evaluateCloseRequest(bool reallyQuit,
        bool closeToTrayPref, bool trayAvail, bool toastShown);

    /// Shared HTTP client used by the app-side image provider and
    /// (later) view-models that fetch their own resources.
    core::HttpClient* httpClient() const { return m_http.get(); }
    /// Shared image loader the QML image provider proxies for.
    ui::ImageLoader* imageLoader() const { return m_imageLoader; }
    /// The QML application window, valid after `attachWindow`.
    QQuickWindow* quickWindow() const { return m_window; }

public Q_SLOTS:
    /// Quit the app immediately, bypassing the close-to-tray
    /// branch. Wired to the drawer's Quit action and `Ctrl+Q`.
    void requestQuit();
    /// Drawer Settings action / `Ctrl+,`. Phase 02 only emits the
    /// signal; ApplicationShell.qml pushes the placeholder page.
    /// Phase 06 swaps in the real `Kirigami.CategorizedSettings`.
    void requestSettings();
    /// Drawer About action / `F1`. ApplicationShell pushes
    /// `KAboutPage` which reads `aboutData` from this controller.
    void requestAbout();
    /// `Ctrl+F`. ApplicationShell tries `focusSearchField()` on
    /// the current page first; falls back to navigating to the
    /// Search page so the field is reachable from any context.
    void requestFocusSearch();

    /// Routed from a Discover rail's "Show all →" via
    /// `DiscoverViewModel::browseRequested`. Applies the preset
    /// to `BrowseViewModel` and asks the QML shell to navigate.
    void applyBrowsePreset(int kind, int sort);

    /// Open the movie detail page for `imdbId`. Loads the VM
    /// against the id and emits `showMovieDetailRequested` so
    /// `ApplicationShell.qml` pushes the detail page on top of the
    /// current Discover / Search / Browse base. Title is forwarded
    /// for the in-flight "Looking up …" status message only.
    void openMovieDetail(const QString& imdbId, const QString& title);
    /// Same shape, but resolves a TMDB id to the IMDB id first
    /// (Browse / Discover hand off TMDB ids).
    void openMovieDetailByTmdb(int tmdbId, const QString& title);

    /// Series counterparts. The 4-argument overload accepts a
    /// season + episode seed (Continue Watching for a series
    /// episode) so the page lands pre-selected on the right row.
    void openSeriesDetail(const QString& imdbId, const QString& title);
    void openSeriesDetailAt(const QString& imdbId, const QString& title,
        int season, int episode);
    void openSeriesDetailByTmdb(int tmdbId, const QString& title);

    /// Called from QML's `onClosing` handler. Returns true when
    /// the close should proceed (real quit), false when the
    /// controller has already hidden the window and the close
    /// must be cancelled (`close.accepted = false`).
    bool handleWindowCloseRequested();

Q_SIGNALS:
    void showSettingsRequested();
    void showAboutRequested();
    void focusSearchRequested();
    /// Sent after `applyBrowsePreset` so the shell can swap the
    /// page row to Browse with the freshly-applied filters.
    void navigateToBrowseRequested();
    /// Asks `ApplicationShell.qml` to push the matching detail
    /// page on top of the current nav page. The view-model has
    /// already been told to load by the time the signal fires; the
    /// QML shell just needs to surface the page so its bindings
    /// paint.
    void showMovieDetailRequested();
    void showSeriesDetailRequested();

    /// Fan-in for status messages coming out of `StreamActions`,
    /// `PlayerLauncher`, `SubtitleController`, and (from phase 03)
    /// view-models. `ApplicationShell.qml` listens and surfaces
    /// each as a `Kirigami.PassiveNotification` on the main window.
    void passiveMessage(const QString& text, int durationMs);

private:
    void buildCoreServices();
    void registerImageProvider(QQmlApplicationEngine& engine);
    void exposeContextProperties(QQmlApplicationEngine& engine);
    void installLocalizedContext(QQmlApplicationEngine& engine);
    void wireStatusForwarding();
    void wireTray();
#ifdef KINEMA_HAVE_LIBMPV
    /// Lazily build the embedded player window and forward the
    /// play request. Wired to `PlayerLauncher::embeddedRequested`
    /// the same way `MainWindow::openEmbeddedPlayer` was.
    void openEmbeddedPlayer(const QUrl& url,
        const api::PlaybackContext& ctx);
#endif

    config::AppSettings& m_settings;
    QQuickWindow* m_window = nullptr;

    // Owned services (non-QObject parented because their lifetime
    // is governed by `unique_ptr`s on this controller).
    std::unique_ptr<core::HttpClient> m_http;
    std::unique_ptr<core::TokenStore> m_tokens;
    std::unique_ptr<core::PlayerLauncher> m_player;
    std::unique_ptr<core::Database> m_db;
    std::unique_ptr<core::HistoryStore> m_history;
    std::unique_ptr<core::SubtitleCacheStore> m_subtitleCache;

    // QObject-parented to this controller.
    api::CinemetaClient* m_cinemeta {};
    api::TorrentioClient* m_torrentio {};
    api::TmdbClient* m_tmdb {};
    api::OpenSubtitlesClient* m_openSubtitles {};
    ui::ImageLoader* m_imageLoader {};

    services::StreamActions* m_streamActions {};
    controllers::TokenController* m_tokenCtrl {};
    controllers::HistoryController* m_historyCtrl {};
    controllers::SubtitleController* m_subtitleCtrl {};
    controllers::TrayController* m_tray {};

    // Per-page view-models. Phase 03 adds Discover + Continue
    // Watching; phases 04–06 add Search / Browse / Detail /
    // Subtitles / Settings here as their pages land.
    DiscoverViewModel* m_discoverVm {};
    ContinueWatchingViewModel* m_continueWatchingVm {};
    SearchViewModel* m_searchVm {};
    BrowseViewModel* m_browseVm {};
    MovieDetailViewModel* m_movieDetailVm {};
    SeriesDetailViewModel* m_seriesDetailVm {};
#ifdef KINEMA_HAVE_LIBMPV
    controllers::PlaybackController* m_playbackCtrl {};
    ui::player::PlayerWindow* m_playerWindow {};
#endif

    // Set by `requestQuit()` so a subsequent close request short-
    // circuits the hide-to-tray branch.
    bool m_reallyQuit = false;
    // One-shot per-session: only the first hide-to-tray emits the
    // "still running in tray" toast.
    bool m_hasShownTrayToast = false;
};

} // namespace kinema::ui::qml
