// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include "api/PlaybackContext.h"
#include "core/MpvChapterList.h"
#include "core/MpvTrackList.h"

#include <QPointer>
#include <QString>
#include <QStringList>
#include <QUrl>
#include <QtQuick/QQuickView>

class QWidget;

namespace kinema::config {
class AppearanceSettings;
class PlayerSettings;
}

namespace kinema::ui::player {

class MpvVideoItem;
class PlayerViewModel;

/**
 * Top-level window that hosts the embedded player. Subclasses
 * `QQuickView` so the entire player surface (video + chrome) lives
 * in one Qt Quick scene graph backed by a single Wayland surface.
 *
 * Public slots / signals are preserved verbatim from the previous
 * QWidget-based implementation so `PlaybackController`,
 * `TrayController`, `HistoryController`, and `MainWindow` are not
 * touched. Internally, the slots forward to `PlayerViewModel`
 * (chrome state) and `MpvVideoItem` (transport).
 *
 * Geometry persistence and remembered-volume behaviour matches the
 * old window: applied on first show, saved on every hide, never
 * destroyed between plays so the libmpv context stays warm.
 */
class PlayerWindow : public QQuickView
{
    Q_OBJECT
public:
    /// `parent` is the `QWidget` that owns the lifetime; we grab
    /// its `windowHandle()` and use it as the transient parent so
    /// the window manager places the player relative to the main
    /// window. `QQuickView` only accepts a `QWindow*` parent, so a
    /// QWidget owner is wired via `setTransientParent` instead.
    explicit PlayerWindow(config::AppearanceSettings& appearance,
        config::PlayerSettings& player,
        QWidget* parent = nullptr);
    ~PlayerWindow() override;

    PlayerWindow(const PlayerWindow&) = delete;
    PlayerWindow& operator=(const PlayerWindow&) = delete;

    /// Load `url` into mpv and show the window.
    virtual void play(const QUrl& url,
        const kinema::api::PlaybackContext& ctx);

    // Imperative control surface (matches the previous public API).
    virtual void setPaused(bool paused);
    virtual void seekAbsolute(double seconds);
    virtual void setAudioTrack(int id);
    virtual void setSubtitleTrack(int id);
    /// Programmatic playback-speed setter, used by
    /// `PlaybackController` when the QML speed picker emits.
    virtual void setSpeed(double factor);
    virtual void showResumePrompt(qint64 seconds);
    virtual void hideResumePrompt();
    virtual void showNextEpisodeBanner(
        const kinema::api::PlaybackContext& ctx, int countdownSec);
    virtual void updateNextEpisodeCountdown(int seconds);
    virtual void hideNextEpisodeBanner();
    virtual void showSkipChapter(const QString& kind,
        const QString& label, qint64 startSec, qint64 endSec);
    virtual void hideSkipChapter();

    /// Stop playback, leave fullscreen if needed, hide the window.
    void stopAndHide();

    bool hasEverLoaded() const noexcept { return m_hasEverLoaded; }

    /// Track-list snapshot (used by `HistoryController` to remember
    /// the user's selected audio / subtitle language). Empty when
    /// no file has loaded yet.
    const core::tracks::TrackList& trackList() const;
    /// Most recent log lines from libmpv. Used by
    /// `PlaybackController` to classify end-file errors.
    QStringList recentLogLines() const;

Q_SIGNALS:
    // Playback signals (forwarded from MpvVideoItem).
    void fileLoaded();
    void endOfFile(const QString& reason);
    void mpvError(const QString& message);
    void positionChanged(double seconds);
    void durationChanged(double seconds);
    void trackListChanged(const core::tracks::TrackList& tracks);
    void chaptersChanged(const core::chapters::ChapterList& chapters);
    /// Emitted from showEvent / hideEvent so MainWindow can refresh
    /// the tray context menu.
    void visibilityChanged(bool visible);

    // User-action signals re-emitted from PlayerViewModel.
    // PlaybackController consumes these eight to drive the resume,
    // skip-chapter, next-episode, and picker flows.
    void resumeAccepted();
    void resumeDeclined();
    void skipRequested();
    void nextEpisodeAccepted();
    void nextEpisodeCancelled();
    void audioPicked(int trackId);
    void subtitlePicked(int trackId);
    void speedPicked(double factor);

private Q_SLOTS:
    /// Mirror QML's `chromeVisible` to the QQuickWindow cursor:
    /// blank cursor when chrome is hidden, default cursor when it
    /// is visible. Connected to the root item's
    /// `chromeVisibleChanged` notify signal after `setSource`.
    void onChromeVisibleChanged();

protected:
    void closeEvent(QCloseEvent* e) override;
    void keyPressEvent(QKeyEvent* e) override;
    void showEvent(QShowEvent* e) override;
    void hideEvent(QHideEvent* e) override;

private:
    void toggleFullscreen();
    void leaveFullscreenOrClose();
    void loadGeometry();
    void saveGeometryToConfig();
    void saveVolumeToConfig();
    void pushMediaChips();
    void pushCheatSheetText();

    config::AppearanceSettings& m_appearanceSettings;
    config::PlayerSettings& m_playerSettings;

    PlayerViewModel* m_viewModel = nullptr;
    QPointer<MpvVideoItem> m_video;
    QWidget* m_widgetParent = nullptr;

    // Public-API stability check: zero-overhead empty fallback
    // returned from `trackList()` / `recentLogLines()` when no
    // video item is wired yet.
    core::tracks::TrackList m_emptyTracks;

    bool m_hasEverLoaded = false;
    bool m_geometryApplied = false;
};

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV
