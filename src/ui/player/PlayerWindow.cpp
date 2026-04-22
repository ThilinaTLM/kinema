// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "ui/player/PlayerWindow.h"

#include "ui/player/MpvWidget.h"
#include "ui/player/widgets/PlayerOverlay.h"
#include "ui/player/widgets/TransportBar.h"

#include "config/AppearanceSettings.h"
#include "config/PlayerSettings.h"

#include <KLocalizedString>

#include <QByteArray>
#include <QCloseEvent>
#include <QGuiApplication>
#include <QHideEvent>
#include <QIcon>
#include <QKeyEvent>
#include <QResizeEvent>
#include <QScreen>
#include <QShowEvent>
#include <QString>
#include <QVBoxLayout>
#include <QWidget>

namespace kinema::ui::player {

PlayerWindow::PlayerWindow(config::AppearanceSettings& appearance,
    config::PlayerSettings& player, QWidget* parent)
    : QWidget(parent, Qt::Window)
    , m_appearanceSettings(appearance)
    , m_playerSettings(player)
{
    // App-consistent window chrome. The actual title is set per-play
    // by play(); the placeholder here is what shows if someone
    // toggles the window visible before loading a file.
    setWindowTitle(i18nc("@title:window", "Kinema Player"));
    setWindowIcon(QIcon::fromTheme(QStringLiteral("dev.tlmtech.kinema")));

    // Full-bleed video. The MpvWidget fills the window; the Qt
    // overlay (transport bar + buffering spinner) sits on top of it
    // as a sibling, positioned manually in resizeEvent so mpv's
    // OpenGL content and the overlay don't fight over a single
    // layout slot.
    auto* layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    m_mpv = new MpvWidget(m_playerSettings, this);
    layout->addWidget(m_mpv);

    m_overlay = new widgets::PlayerOverlay(m_mpv, this);
    m_overlay->raise();
    m_overlay->show();

    connect(m_overlay, &widgets::PlayerOverlay::toggleFullscreenRequested,
        this, &PlayerWindow::toggleFullscreen);
    connect(m_overlay, &widgets::PlayerOverlay::closeRequested,
        this, [this] { close(); });

    // Route mpv-side signals up to MainWindow (KNotification / status
    // bar) and into our own fullscreen/close handling.
    connect(m_mpv, &MpvWidget::fileLoaded,
        this, &PlayerWindow::fileLoaded);
    connect(m_mpv, &MpvWidget::mpvError,
        this, &PlayerWindow::mpvError);
    connect(m_mpv, &MpvWidget::endOfFile,
        this, [this](const QString& reason) {
            // Window has already served its purpose — stop + hide,
            // then let MainWindow react to the reason (e.g. show an
            // "ended with an error" status message).
            stopAndHide();
            Q_EMIT endOfFile(reason);
        });
    connect(m_mpv, &MpvWidget::toggleFullscreenRequested,
        this, &PlayerWindow::toggleFullscreen);
    connect(m_mpv, &MpvWidget::leaveFullscreenRequested,
        this, &PlayerWindow::leaveFullscreenOrClose);
    connect(m_mpv, &MpvWidget::fullscreenChanged,
        this, &PlayerWindow::mirrorMpvFullscreen);
    // Re-emit progress / track metadata signals up to controllers.
    connect(m_mpv, &MpvWidget::positionChanged,
        this, &PlayerWindow::positionChanged);
    connect(m_mpv, &MpvWidget::durationChanged,
        this, &PlayerWindow::durationChanged);
    connect(m_mpv, &MpvWidget::trackListChanged,
        this, &PlayerWindow::trackListChanged);
    connect(m_mpv, &MpvWidget::chaptersChanged,
        this, &PlayerWindow::chaptersChanged);
}

PlayerWindow::~PlayerWindow() = default;

void PlayerWindow::play(const QUrl& url, const api::PlaybackContext& ctx)
{
    m_hasEverLoaded = true;

    const auto& title = ctx.title;
    setWindowTitle(title.isEmpty()
        ? QStringLiteral("Kinema")
        : i18nc("@title:window window title with media title",
              "%1 — Kinema", title));

    // On first show, apply persisted or default geometry before the
    // window actually becomes visible (showEvent does this via
    // m_geometryApplied, but show() also triggers showEvent, so
    // setting geometry here ahead of show() is belt-and-braces for
    // the case where the window is already visible and play() is
    // just swapping files).
    if (!m_geometryApplied) {
        loadGeometry();
    }

    std::optional<double> startSec;
    if (ctx.resumeSeconds && *ctx.resumeSeconds > 0) {
        startSec = static_cast<double>(*ctx.resumeSeconds);
    }
    // Seed the title bar with the new context *before* we kick mpv,
    // so the first visible frame of chrome already reflects the new
    // media instead of the previous session's title.
    if (m_overlay) {
        m_overlay->setContext(ctx);
    }
    m_mpv->loadFile(url, startSec);

    show();
    raise();
    activateWindow();
    m_mpv->setFocus();
}

void PlayerWindow::setPaused(bool paused)
{
    if (m_mpv) {
        m_mpv->setPaused(paused);
    }
}

void PlayerWindow::seekAbsolute(double seconds)
{
    if (m_mpv) {
        m_mpv->seekAbsolute(seconds);
    }
}

void PlayerWindow::setAudioTrack(int id)
{
    if (m_mpv) {
        m_mpv->setAudioTrack(id);
    }
}

void PlayerWindow::setSubtitleTrack(int id)
{
    if (m_mpv) {
        m_mpv->setSubtitleTrack(id);
    }
}

void PlayerWindow::showResumePrompt(qint64 seconds)
{
    if (m_overlay) {
        m_overlay->showResumePrompt(seconds);
    }
}

void PlayerWindow::hideResumePrompt()
{
    if (m_overlay) {
        m_overlay->hideResumePrompt();
    }
}

void PlayerWindow::showNextEpisodeBanner(
    const api::PlaybackContext& ctx, int countdownSec)
{
    if (m_overlay) {
        m_overlay->showNextEpisodeBanner(ctx, countdownSec);
    }
}

void PlayerWindow::updateNextEpisodeCountdown(int seconds)
{
    if (m_overlay) {
        m_overlay->updateNextEpisodeCountdown(seconds);
    }
}

void PlayerWindow::hideNextEpisodeBanner()
{
    if (m_overlay) {
        m_overlay->hideNextEpisodeBanner();
    }
}

void PlayerWindow::showSkipChapter(const QString& label)
{
    if (m_overlay) {
        m_overlay->showSkipChapter(label);
    }
}

void PlayerWindow::hideSkipChapter()
{
    if (m_overlay) {
        m_overlay->hideSkipChapter();
    }
}

void PlayerWindow::stopAndHide()
{
    if (isFullScreen()) {
        // Leave fullscreen first so the next show() uses the
        // remembered windowed geometry, not the bare screen.
        m_applyingFullscreen = true;
        showNormal();
        if (m_mpv) {
            m_mpv->setMpvFullscreen(false);
        }
        m_applyingFullscreen = false;
    }
    if (m_overlay) {
        m_overlay->resetForNewSession();
    }
    if (m_mpv) {
        m_mpv->stop();
    }
    hide();
}

void PlayerWindow::closeEvent(QCloseEvent* e)
{
    // X button / window manager close. Persist geometry, stop
    // playback, hide. Never destroy — we want to reuse the libmpv
    // context for the next play.
    saveGeometryToConfig();
    stopAndHide();
    e->accept();
}

void PlayerWindow::keyPressEvent(QKeyEvent* e)
{
    if (e->key() == Qt::Key_Escape) {
        if (m_overlay && m_overlay->cancelNextEpisodeBanner()) {
            e->accept();
            return;
        }
        leaveFullscreenOrClose();
        e->accept();
        return;
    }
    if (e->key() == Qt::Key_N && e->modifiers() == Qt::NoModifier
        && m_overlay && m_overlay->acceptNextEpisodeBanner()) {
        e->accept();
        return;
    }
    if (e->key() == Qt::Key_N && (e->modifiers() & Qt::ShiftModifier)
        && m_overlay && m_overlay->cancelNextEpisodeBanner()) {
        e->accept();
        return;
    }
    // `?` and `I` are owned by the overlay (cheat sheet / stats)
    // and must never reach mpv's input.conf. The overlay's event
    // filter catches them when focus sits on MpvWidget; when focus
    // is on the window itself (e.g. immediately after show() before
    // the child grabs focus) we toggle directly.
    const bool questionMarked
        = e->key() == Qt::Key_Question
        || (e->key() == Qt::Key_Slash
            && (e->modifiers() & Qt::ShiftModifier));
    if (questionMarked && m_overlay) {
        m_overlay->toggleCheatSheet();
        e->accept();
        return;
    }
    if (e->key() == Qt::Key_I && e->modifiers() == Qt::NoModifier
        && m_overlay) {
        m_overlay->toggleStats();
        e->accept();
        return;
    }
    QWidget::keyPressEvent(e);
}

void PlayerWindow::showEvent(QShowEvent* e)
{
    if (!m_geometryApplied) {
        loadGeometry();
    }
    QWidget::showEvent(e);
    Q_EMIT visibilityChanged(true);
}

void PlayerWindow::hideEvent(QHideEvent* e)
{
    // Persist on every hide (covers both the close-button path and
    // explicit hide() calls from stopAndHide()), so crashes between
    // open and quit don't lose the user's sizing or volume.
    saveGeometryToConfig();
    saveVolumeToConfig();
    QWidget::hideEvent(e);
    Q_EMIT visibilityChanged(false);
}

void PlayerWindow::resizeEvent(QResizeEvent* e)
{
    QWidget::resizeEvent(e);
    if (m_overlay) {
        // Keep the overlay pinned to the video surface. Geometry is
        // the MpvWidget's rect expressed in our own coordinates.
        m_overlay->setGeometry(m_mpv->geometry());
        m_overlay->raise();
    }
}

void PlayerWindow::toggleFullscreen()
{
    if (m_applyingFullscreen) {
        return;
    }
    m_applyingFullscreen = true;
    if (isFullScreen()) {
        showNormal();
        if (m_mpv) {
            m_mpv->setMpvFullscreen(false);
        }
    } else {
        showFullScreen();
        if (m_mpv) {
            m_mpv->setMpvFullscreen(true);
        }
    }
    if (m_mpv) {
        m_mpv->setFocus();
    }
    m_applyingFullscreen = false;
}

void PlayerWindow::leaveFullscreenOrClose()
{
    // Esc / mpv "leave fullscreen" request. Windowed → close the
    // window (which in turn stops playback and hides). Fullscreen
    // → just leave fullscreen, keep playing.
    if (isFullScreen()) {
        m_applyingFullscreen = true;
        showNormal();
        if (m_mpv) {
            m_mpv->setMpvFullscreen(false);
            m_mpv->setFocus();
        }
        m_applyingFullscreen = false;
        return;
    }
    close();
}

void PlayerWindow::mirrorMpvFullscreen(bool on)
{
    // mpv's `fullscreen` property flipped (e.g. user's own script or
    // OSC toggle). Mirror into Qt. The re-entrancy guard suppresses
    // the feedback loop when *we* were the one who set it.
    if (m_applyingFullscreen) {
        return;
    }
    m_applyingFullscreen = true;
    if (on && !isFullScreen()) {
        showFullScreen();
    } else if (!on && isFullScreen()) {
        showNormal();
    }
    m_applyingFullscreen = false;
}

void PlayerWindow::loadGeometry()
{
    m_geometryApplied = true;

    const QByteArray saved = m_appearanceSettings.playerWindowGeometry();
    if (!saved.isEmpty() && restoreGeometry(saved)) {
        return;
    }

    // First-run default: 1280x720 centred on the parent's screen
    // (falls back to the primary screen if we have no parent).
    resize(1280, 720);
    QScreen* screen = nullptr;
    if (auto* p = parentWidget()) {
        screen = p->screen();
    }
    if (!screen) {
        screen = QGuiApplication::primaryScreen();
    }
    if (screen) {
        const QRect avail = screen->availableGeometry();
        move(avail.center() - rect().center());
    }
}

void PlayerWindow::saveGeometryToConfig()
{
    m_appearanceSettings.setPlayerWindowGeometry(saveGeometry());
}

void PlayerWindow::saveVolumeToConfig()
{
    if (!m_mpv) {
        return;
    }
    const double v = m_mpv->currentVolume();
    // -1.0 in MpvWidget means "no volume sample observed yet" — don't
    // overwrite a previously-saved value with junk in that case.
    if (v < 0.0) {
        return;
    }
    m_playerSettings.setRememberedVolume(v);
}

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV
