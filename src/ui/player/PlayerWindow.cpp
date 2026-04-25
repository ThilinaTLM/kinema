// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "ui/player/PlayerWindow.h"

#include "ui/player/MpvWidget.h"

#include "config/AppearanceSettings.h"
#include "config/PlayerSettings.h"
#include "core/CheatSheetText.h"
#include "core/MediaChips.h"
#include "core/MpvIpc.h"
#include "core/MpvTrackList.h"

#include <KLocalizedString>

#include <QByteArray>
#include <QCloseEvent>
#include <QGuiApplication>
#include <QHideEvent>
#include <QIcon>
#include <QKeyEvent>
#include <QLatin1Char>
#include <QScreen>
#include <QShowEvent>
#include <QString>
#include <QVBoxLayout>
#include <QWidget>

namespace kinema::ui::player {

namespace {

/// Build the short subtitle line the title strip renders under the
/// primary title. Movies have no subtitle; series get
/// `"S01E03 \u2014 Episode title"` when we know both the key and the
/// episode title, falling back to the numeric part alone otherwise.
QString buildSubtitleLabel(const api::PlaybackContext& ctx)
{
    if (ctx.key.kind != api::MediaKind::Series) {
        return {};
    }
    if (!ctx.key.season.has_value() || !ctx.key.episode.has_value()) {
        return ctx.episodeTitle;
    }
    const QString code = QStringLiteral("S%1E%2")
                             .arg(*ctx.key.season, 2, 10, QLatin1Char('0'))
                             .arg(*ctx.key.episode, 2, 10, QLatin1Char('0'));
    if (ctx.episodeTitle.isEmpty()) {
        return code;
    }
    return i18nc(
        "@label subtitle on the in-mpv title strip, e.g. "
        "\"S01E03 \u2014 Episode Title\"",
        "%1 \u2014 %2", code, ctx.episodeTitle);
}

} // namespace

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

    // Full-bleed video. MpvWidget is the *only* child of this window
    // \u2014 all player chrome is rendered inside mpv by the
    // `kinema-overlays` Lua script. Parenting any Qt widget to
    // MpvWidget is now banned; see the "Embedded player chrome"
    // section in AGENTS.md.
    auto* layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    m_mpv = new MpvWidget(m_playerSettings, this);
    layout->addWidget(m_mpv);

    // Wire IPC \u2194 window-level actions the Lua script can request
    // (close, fullscreen toggle). These always go to the PlayerWindow
    // rather than the controller because they manipulate Qt window
    // state.
    if (auto* ipc = m_mpv->ipc()) {
        connect(ipc, &core::MpvIpc::closeRequested,
            this, [this] { close(); });
        connect(ipc, &core::MpvIpc::fullscreenToggleRequested,
            this, &PlayerWindow::toggleFullscreen);
    }

    // Track-list changes feed the Lua pickers. Serialise to compact
    // JSON and push on every change \u2014 mpv emits track-list edits
    // sparsely, and the script's picker re-renders only when open.
    // We also rebuild the media-info chip row whenever the track
    // list changes so codec / audio chips stay in sync with the
    // selected tracks.
    connect(m_mpv, &MpvWidget::trackListChanged,
        this, [this](const core::tracks::TrackList& tracks) {
            if (auto* ipc = m_mpv->ipc()) {
                ipc->setTracks(core::tracks::toIpcJson(tracks));
            }
            pushMediaChips();
        });

    // Video / audio params and codec strings drive the chip row too.
    // MpvWidget emits `videoStatsChanged` whenever any observed
    // property in the stats struct updates; piggyback on it instead
    // of adding a dedicated signal.
    connect(m_mpv, &MpvWidget::videoStatsChanged,
        this, [this](const MpvWidget::VideoStats&) {
            pushMediaChips();
        });

    // (The `palette` IPC path is retired; the Lua script bakes a
    // curated HUD palette in `theme.lua`.)
    // Route mpv-side signals up to MainWindow (KNotification / status
    // bar) and into our own fullscreen/close handling.
    connect(m_mpv, &MpvWidget::fileLoaded,
        this, &PlayerWindow::fileLoaded);
    connect(m_mpv, &MpvWidget::mpvError,
        this, &PlayerWindow::mpvError);
    connect(m_mpv, &MpvWidget::endOfFile,
        this, [this](const QString& reason) {
            // Window has already served its purpose \u2014 stop + hide,
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
              "%1 \u2014 Kinema", title));

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

    // Push the media title into mpv's `force-media-title` *before*
    // loadfile so the Lua title strip has the new value ready by the
    // time the first frame arrives. This also keeps mpv's own stats
    // HUD (bound to `I`) consistent with the Qt window title.
    m_mpv->setMediaTitle(ctx.title);
    m_mpv->loadFile(url, startSec);

    // Seed the Lua chrome with the new context, media chips, and
    // cheat sheet. We resend these on every play so a late-loading
    // script picks them up on its first redraw.
    if (auto* ipc = m_mpv->ipc()) {
        ipc->setContext(ctx.title,
            buildSubtitleLabel(ctx),
            ctx.key.kind == api::MediaKind::Series
                ? QStringLiteral("series")
                : QStringLiteral("movie"));
        pushMediaChips();
        pushCheatSheetText();
    }

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
    if (auto* ipc = m_mpv ? m_mpv->ipc() : nullptr) {
        // Duration is read directly from mpv's `duration` property
        // by the Lua script (`mp.observe_property('duration', …)`),
        // so we just send 0 here. The wire arg is preserved for a
        // future "resume at X of Y" affordance that wants the host
        // to dictate the figure.
        ipc->showResume(seconds, 0);
    }
}

void PlayerWindow::hideResumePrompt()
{
    if (auto* ipc = m_mpv ? m_mpv->ipc() : nullptr) {
        ipc->hideResume();
    }
}

void PlayerWindow::showNextEpisodeBanner(
    const api::PlaybackContext& ctx, int countdownSec)
{
    if (auto* ipc = m_mpv ? m_mpv->ipc() : nullptr) {
        ipc->showNextEpisode(ctx.title, buildSubtitleLabel(ctx),
            countdownSec);
    }
}

void PlayerWindow::updateNextEpisodeCountdown(int seconds)
{
    if (auto* ipc = m_mpv ? m_mpv->ipc() : nullptr) {
        ipc->updateNextEpisodeCountdown(seconds);
    }
}

void PlayerWindow::hideNextEpisodeBanner()
{
    if (auto* ipc = m_mpv ? m_mpv->ipc() : nullptr) {
        ipc->hideNextEpisode();
    }
}

void PlayerWindow::showSkipChapter(const QString& kind,
    const QString& label, qint64 startSec, qint64 endSec)
{
    if (auto* ipc = m_mpv ? m_mpv->ipc() : nullptr) {
        ipc->showSkip(kind, label, startSec, endSec);
    }
}

void PlayerWindow::hideSkipChapter()
{
    if (auto* ipc = m_mpv ? m_mpv->ipc() : nullptr) {
        ipc->hideSkip();
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
    if (m_mpv) {
        m_mpv->stop();
    }
    hide();
}

void PlayerWindow::pushMediaChips()
{
    if (!m_mpv) {
        return;
    }
    auto* ipc = m_mpv->ipc();
    if (!ipc) {
        return;
    }
    const auto stats = m_mpv->currentStats();
    core::media_chips::ChipInputs in;
    in.videoHeight   = stats.height;
    in.videoCodec    = stats.videoCodec;
    in.audioCodec    = stats.audioCodec;
    in.audioChannels = stats.audioChannels;
    in.hdrPrimaries  = stats.hdrPrimaries;
    in.hdrGamma      = stats.hdrGamma;
    ipc->setMediaChips(core::media_chips::toIpcJson(in));
}

void PlayerWindow::pushCheatSheetText()
{
    if (auto* ipc = m_mpv ? m_mpv->ipc() : nullptr) {
        ipc->setCheatSheetText(core::cheatsheet::render());
    }
}

void PlayerWindow::closeEvent(QCloseEvent* e)
{
    // X button / window manager close. Persist geometry, stop
    // playback, hide. Never destroy \u2014 we want to reuse the libmpv
    // context for the next play.
    saveGeometryToConfig();
    stopAndHide();
    e->accept();
}

void PlayerWindow::keyPressEvent(QKeyEvent* e)
{
    // The Lua chrome owns `F`, `Esc`, `?`, `I`, `N`, `Shift+N` via
    // mpv's input layer once MpvWidget has focus (play() grabs focus
    // immediately). This override only fires when the PlayerWindow
    // itself has focus (e.g. the very first paint cycle before the
    // child widget receives it), so we only keep the `Esc \u2192 close`
    // fallback \u2014 everything else is expected to reach mpv directly.
    if (e->key() == Qt::Key_Escape) {
        leaveFullscreenOrClose();
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
    // Esc / mpv "leave fullscreen" request. Windowed \u2192 close the
    // window (which in turn stops playback and hides). Fullscreen
    // \u2192 just leave fullscreen, keep playing.
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
    // -1.0 in MpvWidget means "no volume sample observed yet" \u2014 don't
    // overwrite a previously-saved value with junk in that case.
    if (v < 0.0) {
        return;
    }
    m_playerSettings.setRememberedVolume(v);
}

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV
