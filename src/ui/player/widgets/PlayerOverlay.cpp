// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "ui/player/widgets/PlayerOverlay.h"

#include "core/MpvChapterList.h"
#include "ui/player/MpvWidget.h"
#include "ui/player/widgets/BufferingOverlay.h"
#include "ui/player/widgets/CheatSheetOverlay.h"
#include "ui/player/widgets/NextEpisodeBanner.h"
#include "ui/player/widgets/PlayerTitleBar.h"
#include "ui/player/widgets/ResumePrompt.h"
#include "ui/player/widgets/StatsOverlay.h"
#include "ui/player/widgets/TransportBar.h"

#include <QApplication>
#include <QCursor>
#include <QEvent>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QResizeEvent>
#include <QTimer>

namespace kinema::ui::player::widgets {

namespace {

constexpr int kHideDelayMs = 2000;
constexpr int kTransportHeightHint = 64;
constexpr int kTitleBarHeightHint = 48;
constexpr int kBufferingHeightHint = 56;
constexpr int kStatsWidthHint = 260;
constexpr int kStatsHeightHint = 140;
constexpr int kStatsMargin = 12;
constexpr int kResumePromptWidth = 360;
constexpr int kResumePromptHeight = 150;
constexpr int kNextEpisodeWidth = 320;
constexpr int kNextEpisodeHeight = 145;

} // namespace

PlayerOverlay::PlayerOverlay(MpvWidget* mpv, QWidget* parent)
    : QWidget(parent)
    , m_mpv(mpv)
{
    // Overlay itself should not eat clicks; individual sub-widgets
    // (TransportBar / title bar) flip the flag off for their own
    // interactive regions.
    setAttribute(Qt::WA_TransparentForMouseEvents);
    setAttribute(Qt::WA_NoSystemBackground);
    setAutoFillBackground(false);
    setFocusPolicy(Qt::NoFocus);

    m_transport = new TransportBar(mpv, this);
    m_buffering = new BufferingOverlay(this);
    m_titleBar = new PlayerTitleBar(this);
    m_stats = new StatsOverlay(this);
    m_cheatSheet = new CheatSheetOverlay(this);
    m_resumePrompt = new ResumePrompt(this);
    m_nextEpisodeBanner = new NextEpisodeBanner(this);

    // Re-emit the user-driven actions up to PlayerWindow.
    connect(m_transport, &TransportBar::toggleFullscreenRequested,
        this, &PlayerOverlay::toggleFullscreenRequested);
    connect(m_transport, &TransportBar::closeRequested,
        this, &PlayerOverlay::closeRequested);

    // Title bar: close goes to PlayerWindow; selection / speed /
    // overlay-toggle requests route back into mpv or the sub-overlays.
    connect(m_titleBar, &PlayerTitleBar::closeRequested,
        this, &PlayerOverlay::closeRequested);
    connect(m_transport, &TransportBar::skipRequested,
        this, &PlayerOverlay::skipRequested);
    connect(m_resumePrompt, &ResumePrompt::resumeRequested,
        this, &PlayerOverlay::resumeRequested);
    connect(m_resumePrompt, &ResumePrompt::restartRequested,
        this, &PlayerOverlay::restartRequested);
    connect(m_nextEpisodeBanner, &NextEpisodeBanner::playNowRequested,
        this, &PlayerOverlay::nextEpisodeAccepted);
    connect(m_nextEpisodeBanner, &NextEpisodeBanner::cancelRequested,
        this, &PlayerOverlay::nextEpisodeCancelled);
    connect(m_titleBar, &PlayerTitleBar::audioTrackRequested,
        this, [this](int id) {
            if (m_mpv) m_mpv->setAudioTrack(id);
        });
    connect(m_titleBar, &PlayerTitleBar::subtitleTrackRequested,
        this, [this](int id) {
            if (m_mpv) m_mpv->setSubtitleTrack(id);
        });
    connect(m_titleBar, &PlayerTitleBar::speedRequested,
        this, [this](double s) {
            if (m_mpv) m_mpv->setSpeed(s);
        });
    connect(m_titleBar, &PlayerTitleBar::statsToggleRequested,
        this, [this] { m_stats->toggleVisibility(); });
    connect(m_titleBar, &PlayerTitleBar::cheatSheetToggleRequested,
        this, [this] { m_cheatSheet->toggleVisibility(); });

    if (mpv) {
        connect(mpv, &MpvWidget::bufferingChanged,
            m_buffering, &BufferingOverlay::setBuffering);
        // Any mpv activity is user activity from the overlay's point
        // of view (playback started, seek landed), so the chrome
        // should wake up briefly. Position updates are too chatty;
        // we only react to "state" events.
        connect(mpv, &MpvWidget::pausedChanged,
            this, [this](bool) { nudgeActivity(); });
        connect(mpv, &MpvWidget::fileLoaded,
            this, [this] { nudgeActivity(); });
        // Track / chapter list fan-out.
        connect(mpv, &MpvWidget::trackListChanged,
            m_titleBar, &PlayerTitleBar::setTrackList);
        connect(mpv, &MpvWidget::chaptersChanged,
            this, [this](const core::chapters::ChapterList& chapters) {
                m_transport->setChapters(
                    core::chapters::chapterTimes(chapters));
            });
        connect(mpv, &MpvWidget::speedChanged,
            m_titleBar, &PlayerTitleBar::setSpeed);
        // Stats: cache the latest snapshot so the 1 Hz timer paints
        // the current values; StatsOverlay does its own throttling.
        connect(mpv, &MpvWidget::videoStatsChanged,
            m_stats, &StatsOverlay::applyStats);
        mpv->installEventFilter(this);
    }

    m_hideTimer = new QTimer(this);
    m_hideTimer->setSingleShot(true);
    m_hideTimer->setInterval(kHideDelayMs);
    connect(m_hideTimer, &QTimer::timeout,
        this, &PlayerOverlay::onActivityTimeout);

    // Start visible so the user sees the chrome on first play; the
    // timer hides it after the activity window.
    setChromeVisible(true);
    m_hideTimer->start();
}

PlayerOverlay::~PlayerOverlay() = default;

void PlayerOverlay::setFullscreen(bool /*on*/)
{
    // Reserved — title bar / transport geometry is already handled
    // by resizeEvent so fullscreen just re-flows.
}

void PlayerOverlay::setContext(const api::PlaybackContext& ctx)
{
    m_titleBar->setContext(ctx);
}

void PlayerOverlay::showResumePrompt(qint64 seconds)
{
    m_resumePrompt->showPrompt(seconds);
}

void PlayerOverlay::hideResumePrompt()
{
    m_resumePrompt->hidePrompt();
}

void PlayerOverlay::showNextEpisodeBanner(
    const api::PlaybackContext& ctx, int countdownSec)
{
    m_nextEpisodeBanner->showBanner(ctx, countdownSec);
}

void PlayerOverlay::updateNextEpisodeCountdown(int seconds)
{
    m_nextEpisodeBanner->setCountdown(seconds);
}

void PlayerOverlay::hideNextEpisodeBanner()
{
    m_nextEpisodeBanner->hideBanner();
}

bool PlayerOverlay::acceptNextEpisodeBanner()
{
    return m_nextEpisodeBanner->acceptVisibleBanner();
}

bool PlayerOverlay::cancelNextEpisodeBanner()
{
    return m_nextEpisodeBanner->cancelVisibleBanner();
}

void PlayerOverlay::showSkipChapter(const QString& label)
{
    m_transport->showSkipChapter(label);
}

void PlayerOverlay::hideSkipChapter()
{
    m_transport->hideSkipChapter();
}

void PlayerOverlay::toggleCheatSheet()
{
    m_cheatSheet->toggleVisibility();
}

void PlayerOverlay::toggleStats()
{
    m_stats->toggleVisibility();
}

void PlayerOverlay::resetForNewSession()
{
    m_buffering->setBuffering(false, 0);
    m_stats->hide();
    m_cheatSheet->hide();
    m_resumePrompt->hidePrompt();
    m_nextEpisodeBanner->hideBanner();
    m_transport->hideSkipChapter();
    setChromeVisible(true);
    m_hideTimer->start();
}

void PlayerOverlay::resizeEvent(QResizeEvent* e)
{
    QWidget::resizeEvent(e);
    const auto r = rect();

    // Title bar pinned to the top edge.
    m_titleBar->setGeometry(0, 0, r.width(), kTitleBarHeightHint);

    // Transport bar pinned to the bottom edge.
    m_transport->setGeometry(
        0, r.height() - kTransportHeightHint,
        r.width(), kTransportHeightHint);

    // Buffering overlay centred on the full surface.
    const int w = 220;
    const int h = kBufferingHeightHint;
    m_buffering->setGeometry(
        (r.width() - w) / 2, (r.height() - h) / 2, w, h);

    // Stats overlay pinned top-right, below the title bar.
    m_stats->setGeometry(
        r.width() - kStatsWidthHint - kStatsMargin,
        kTitleBarHeightHint + kStatsMargin,
        kStatsWidthHint, kStatsHeightHint);

    // Cheat sheet covers the full surface; its own layout centres
    // the panel.
    m_cheatSheet->setGeometry(r);

    m_resumePrompt->setGeometry(
        (r.width() - kResumePromptWidth) / 2,
        (r.height() - kResumePromptHeight) / 2,
        kResumePromptWidth, kResumePromptHeight);

    m_nextEpisodeBanner->setGeometry(
        r.width() - kNextEpisodeWidth - kStatsMargin,
        r.height() - kTransportHeightHint - kNextEpisodeHeight - kStatsMargin,
        kNextEpisodeWidth, kNextEpisodeHeight);
}

bool PlayerOverlay::eventFilter(QObject* watched, QEvent* e)
{
    if (watched == m_mpv.data()) {
        switch (e->type()) {
        case QEvent::MouseMove:
        case QEvent::Enter:
            nudgeActivity();
            break;
        case QEvent::Leave:
            // Don't instantly hide — leave the timer to decide.
            // Mouse may just be in flight between video and chrome.
            break;
        case QEvent::KeyPress: {
            auto* k = static_cast<QKeyEvent*>(e);
            // `?` arrives as Key_Question on most layouts; on a few
            // it's Key_Slash + ShiftModifier. Handle both.
            const bool questionMarked
                = k->key() == Qt::Key_Question
                || (k->key() == Qt::Key_Slash
                    && (k->modifiers() & Qt::ShiftModifier));
            if (questionMarked) {
                m_cheatSheet->toggleVisibility();
                return true;
            }
            if (k->key() == Qt::Key_I
                && k->modifiers() == Qt::NoModifier) {
                m_stats->toggleVisibility();
                return true;
            }
            if (k->key() == Qt::Key_Escape
                && k->modifiers() == Qt::NoModifier
                && m_nextEpisodeBanner->cancelVisibleBanner()) {
                return true;
            }
            if (k->key() == Qt::Key_N
                && k->modifiers() == Qt::NoModifier
                && m_nextEpisodeBanner->acceptVisibleBanner()) {
                return true;
            }
            if (k->key() == Qt::Key_N
                && (k->modifiers() & Qt::ShiftModifier)
                && m_nextEpisodeBanner->cancelVisibleBanner()) {
                return true;
            }
            break;
        }
        default:
            break;
        }
    }
    return QWidget::eventFilter(watched, e);
}

void PlayerOverlay::onActivityTimeout()
{
    // Only hide the chrome when the pointer is actually over the
    // video. If the user parked the mouse over the transport bar
    // itself we keep it visible indefinitely.
    if (!m_mpv) {
        setChromeVisible(false);
        return;
    }
    const auto gp = QCursor::pos();
    const auto local = m_mpv->mapFromGlobal(gp);
    const bool overVideo = m_mpv->rect().contains(local);
    const bool overTransport
        = m_transport->geometry().contains(m_transport->mapFromGlobal(gp));
    const bool overTitle
        = m_titleBar->geometry().contains(m_titleBar->mapFromGlobal(gp));
    if (overTransport || overTitle) {
        // User is hovering chrome; re-arm and keep visible.
        m_hideTimer->start();
        return;
    }
    if (overVideo) {
        setChromeVisible(false);
        // Also hide the mouse cursor itself while the chrome is
        // gone — matches every other full-screen media app.
        m_mpv->setCursor(Qt::BlankCursor);
    } else {
        // Pointer is outside the player entirely; hide chrome but
        // leave the cursor as-is so other apps aren't surprised.
        setChromeVisible(false);
    }
}

void PlayerOverlay::nudgeActivity()
{
    if (!m_chromeVisible) {
        setChromeVisible(true);
    }
    if (m_mpv) {
        m_mpv->unsetCursor();
    }
    m_hideTimer->start();
}

void PlayerOverlay::setChromeVisible(bool visible)
{
    if (m_chromeVisible == visible) {
        return;
    }
    m_chromeVisible = visible;
    m_transport->setVisible(visible);
    m_titleBar->setVisible(visible);
    // Stats / cheat-sheet are driven by their own toggles; hiding
    // the idle chrome shouldn't dismiss them, and BufferingOverlay
    // is driven by mpv's buffering state.
}

} // namespace kinema::ui::player::widgets

#endif // KINEMA_HAVE_LIBMPV
