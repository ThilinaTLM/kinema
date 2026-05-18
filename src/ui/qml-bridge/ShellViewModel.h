// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <KAboutData>

#include <QObject>
#include <QString>

class QQuickWindow;
class QUrl;

namespace kinema::app {
class ServiceContainer;
}

namespace kinema::domain {
struct PlaybackContext;
}

namespace kinema::ui::player {
class PlayerWindow;
}

namespace kinema::ui::qml {

/**
 * QML-facing shell view-model. Replaces the QML surface that used
 * to live on `MainController`: drawer actions, keyboard-shortcut
 * targets, tray ownership, the close-to-tray decision tree,
 * `KAboutData` feed, the passive-notification fan-in, and (under
 * `KINEMA_HAVE_LIBMPV`) the embedded player window's lifecycle.
 *
 * Service ownership lives in `app::ServiceContainer`; this class
 * receives the container by reference and wires the cross-cutting
 * routing between page view-models and the navigation/status
 * signals listed below. Exposed to QML as the `shell` context
 * property.
 */
class ShellViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString applicationName READ applicationName CONSTANT)
    Q_PROPERTY(KAboutData aboutData READ aboutData CONSTANT)
    Q_PROPERTY(bool hideToTrayEnabled READ hideToTrayEnabled CONSTANT)
    Q_PROPERTY(bool trayAvailable READ trayAvailable CONSTANT)

public:
    explicit ShellViewModel(app::ServiceContainer& services,
        QObject* parent = nullptr);
    ~ShellViewModel() override;

    /// Called by `main.cpp` after `engine.load(...)` succeeds.
    /// Caches the root `QQuickWindow*` and constructs the tray
    /// controller against it.
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
    /// Lets `tst_shell_view_model_close_logic` exercise every
    /// path without building a full ShellViewModel.
    static CloseDecision evaluateCloseRequest(bool reallyQuit,
        bool closeToTrayPref, bool trayAvail, bool toastShown);

    /// The QML application window, valid after `attachWindow`.
    QQuickWindow* quickWindow() const { return m_window; }

public Q_SLOTS:
    /// Quit the app immediately, bypassing the close-to-tray
    /// branch. Wired to the drawer's Quit action and `Ctrl+Q`.
    void requestQuit();
    /// Drawer Settings action / `Ctrl+,`. The `category` argument
    /// lands the page on a specific sub-page (e.g. `"general"`,
    /// `"subtitles"`); empty selects the default.
    void requestSettings(const QString& category = QString());

    /// Push the Subtitles page on top of the current nav stack
    /// against `ctx`. `fromPlayer` toggles attach-on-download
    /// semantics so the embedded player picks up the file.
    void pushSubtitlesPage(const domain::PlaybackContext& ctx,
        bool fromPlayer);
    /// Drawer About action / `F1`.
    void requestAbout();
    /// `Ctrl+F`.
    void requestFocusSearch();

    /// Routed from a Discover rail's "Show all →".
    void applyBrowsePreset(int kind, int sort);

    /// Open the movie detail page for `imdbId`.
    void openMovieDetail(const QString& imdbId, const QString& title);
    /// Same shape, but resolves a TMDB id to the IMDB id first.
    void openMovieDetailByTmdb(int tmdbId, const QString& title);

    /// Cross-cutting clipboard / external-URL helpers used by
    /// context menus throughout the app. They live on the shell
    /// (rather than per-VM) so every menu can reach the same
    /// behaviour via the `shell` context property.
    ///
    /// `copyToClipboard` copies `text` to the system clipboard and
    /// surfaces a passive notification. Pass `confirmMessage` to
    /// customise the toast (e.g. "Title copied"); empty falls back
    /// to a generic "Copied to clipboard" message. No-ops on empty
    /// `text`.
    void copyToClipboard(const QString& text,
        const QString& confirmMessage = QString());

    /// Hand `url` off to the desktop's default handler via
    /// `core::io::openExternal`. Surfaces success / failure as a
    /// passive notification. No-ops on invalid URLs.
    void openExternalUrl(const QUrl& url);

    /// Open `https://www.imdb.com/title/<imdbId>/` in the default
    /// browser. No-ops on empty or malformed ids.
    void openImdbTitle(const QString& imdbId);

    /// Open `https://www.themoviedb.org/{movie|tv}/<tmdbId>` in the
    /// default browser. `kind` is `domain::MediaKind` (0 = movie,
    /// 1 = series); other values fall back to the movie path.
    void openTmdbTitle(int tmdbId, int kind);

    /// Context-menu "Find Streams" navigation. Each loads the
    /// matching detail VM (movie or series) by IMDb id or TMDB id
    /// and pushes `StreamsPage`, mirroring the Continue Watching
    /// streams shortcut.
    void findMovieStreamsByImdb(const QString& imdbId,
        const QString& title);
    void findSeriesStreamsByImdb(const QString& imdbId,
        const QString& title);
    void findMovieStreamsByTmdb(int tmdbId, const QString& title);
    void findSeriesStreamsByTmdb(int tmdbId, const QString& title);

    /// Series counterparts.
    void openSeriesDetail(const QString& imdbId, const QString& title);
    void openSeriesDetailAt(const QString& imdbId, const QString& title,
        int season, int episode);
    void openSeriesDetailByTmdb(int tmdbId, const QString& title);

    /// Called from QML's `onClosing` handler. Returns true when
    /// the close should proceed.
    bool handleWindowCloseRequested();

Q_SIGNALS:
    void showSettingsRequested(const QString& category);
    void showSubtitlesRequested();
    void popPageRequested();
    void showAboutRequested();
    void focusSearchRequested();
    void navigateToBrowseRequested();
    void showMovieDetailRequested();
    void showSeriesDetailRequested();
    void showStreamsRequested(QObject* detailVm);
    /// Fan-in for status messages coming out of `StreamActions`,
    /// `PlayerLauncher`, `SubtitleController`, and view-models.
    /// `ApplicationShell.qml` listens and surfaces each as a
    /// `Kirigami.PassiveNotification`.
    void passiveMessage(const QString& text, int durationMs);

private:
    void wireNavigationRouting();
    void wireStatusForwarding();
    void wireTray();
#ifdef KINEMA_HAVE_LIBMPV
    /// Lazily build the embedded player window the first time it
    /// is needed and reuse the same instance for subsequent plays.
    ui::player::PlayerWindow* ensurePlayerWindow();

    /// Forward a play request to the embedded window. Used by the
    /// `PlayerLauncher::embeddedRequested` connection.
    void openEmbeddedPlayer(const QUrl& url,
        const domain::PlaybackContext& ctx);

    /// Refresh the tray menu when the player window's visibility
    /// changes.
    void onPlayerVisibilityChanged(bool visible);
#endif

    app::ServiceContainer& m_services;
    QQuickWindow* m_window = nullptr;

#ifdef KINEMA_HAVE_LIBMPV
    ui::player::PlayerWindow* m_playerWindow {};
#endif

    /// Set by `requestQuit()` so a subsequent close request short-
    /// circuits the hide-to-tray branch.
    bool m_reallyQuit = false;
    /// One-shot per-session: only the first hide-to-tray emits
    /// the "still running in tray" toast.
    bool m_hasShownTrayToast = false;
};

} // namespace kinema::ui::qml
