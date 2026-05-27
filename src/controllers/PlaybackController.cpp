// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/PlaybackController.h"

#ifdef KINEMA_HAVE_LIBMPV

#include "config/AppSettings.h"
#include "core/io/UrlRedactor.h"
#include "playback/session/PlayerLoadWatchdog.h"
#include "ui/player/PlayerWindow.h"
#include "kinema_log_controller.h"

#include <KLocalizedString>

#include <QRegularExpression>

#include <algorithm>

namespace kinema::controllers {

bool PlaybackController::isActivelyPlaying() const noexcept
{
    return m_hasActiveSession
        && m_phase == Phase::Playing
        && !m_paused;
}

namespace {

const QRegularExpression kSkipRx(
    QStringLiteral("^(intro|opening|outro|ending|credits|end credits)\\b"),
    QRegularExpression::CaseInsensitiveOption);
const QRegularExpression kCreditsRx(
    QStringLiteral("^(credits|end credits)\\b"),
    QRegularExpression::CaseInsensitiveOption);
const QRegularExpression kOutroRx(
    QStringLiteral("^(outro|ending)\\b"),
    QRegularExpression::CaseInsensitiveOption);

enum class SkipKind { Intro, Outro, Credits };

SkipKind classifySkipChapter(const QString& chapterTitle)
{
    const QString t = chapterTitle.trimmed();
    if (t.contains(kCreditsRx)) {
        return SkipKind::Credits;
    }
    if (t.contains(kOutroRx)) {
        return SkipKind::Outro;
    }
    return SkipKind::Intro;
}

QString skipButtonLabel(SkipKind kind)
{
    switch (kind) {
    case SkipKind::Credits:
        return i18nc("@action:button", "Skip credits");
    case SkipKind::Outro:
        return i18nc("@action:button", "Skip outro");
    case SkipKind::Intro:
        break;
    }
    return i18nc("@action:button", "Skip intro");
}

QString skipChapterKind(SkipKind kind)
{
    switch (kind) {
    case SkipKind::Credits:
        return QStringLiteral("credits");
    case SkipKind::Outro:
        return QStringLiteral("outro");
    case SkipKind::Intro:
        break;
    }
    return QStringLiteral("intro");
}

} // namespace

PlaybackController::PlaybackController(
    const config::AppSettings& settings,
    QObject* parent)
    : QObject(parent)
    , m_settings(settings)
    , m_loadWatchdog(new playback::session::PlayerLoadWatchdog(this))
{
    connect(m_loadWatchdog,
        &playback::session::PlayerLoadWatchdog::timedOut,
        this, &PlaybackController::onLoadWatchdogTimedOut);
}

void PlaybackController::setPlayerWindow(ui::player::PlayerWindow* window)
{
    if (m_window == window) {
        return;
    }
    if (m_window) {
        disconnect(m_window, nullptr, this, nullptr);
    }
    m_window = window;
    if (!m_window) {
        m_loadWatchdog->stop();
        m_phase = Phase::Idle;
        m_hasActiveSession = false;
        m_paused = false;
        m_position = 0.0;
        m_duration = 0.0;
        // Window detach: any in-flight loadfile transition is
        // cancelled along with the player surface.
        m_loadfileInFlight = false;
        m_loadedCtx = {};
        Q_EMIT sessionStateChanged();
        return;
    }

    connect(m_window, &ui::player::PlayerWindow::fileLoaded,
        this, &PlaybackController::onFileLoaded);
    connect(m_window, &ui::player::PlayerWindow::mpvError,
        this, &PlaybackController::onPlaybackError);
    connect(m_window, &ui::player::PlayerWindow::endOfFile,
        this, &PlaybackController::onEndOfFile);
    connect(m_window, &ui::player::PlayerWindow::positionChanged,
        this, &PlaybackController::onPositionChanged);
    connect(m_window, &ui::player::PlayerWindow::durationChanged,
        this, &PlaybackController::onDurationChanged);
    connect(m_window, &ui::player::PlayerWindow::pausedChanged,
        this, &PlaybackController::onPausedChanged);
    connect(m_window, &ui::player::PlayerWindow::volumeChanged,
        this, &PlaybackController::onVolumeChanged);
    connect(m_window, &ui::player::PlayerWindow::speedChanged,
        this, &PlaybackController::onSpeedChanged);
    connect(m_window, &ui::player::PlayerWindow::visibilityChanged,
        this, &PlaybackController::visibilityChanged);
    connect(m_window, &ui::player::PlayerWindow::chaptersChanged,
        this, &PlaybackController::onChaptersChanged);
    // User-action signals from the QML chrome reach us as plain
    // signals on `PlayerWindow` (which re-emits them from its
    // `PlayerViewModel`). The picker selections (audio / subtitle /
    // speed) get routed back through the window's transport API
    // so tests can stub `PlayerWindow` without needing libmpv.
    connect(m_window, &ui::player::PlayerWindow::skipRequested,
        this, &PlaybackController::onSkipRequested);
    connect(m_window, &ui::player::PlayerWindow::resumeAccepted,
        this, &PlaybackController::onResumeAccepted);
    connect(m_window, &ui::player::PlayerWindow::resumeDeclined,
        this, &PlaybackController::onResumeDeclined);
    connect(m_window, &ui::player::PlayerWindow::audioPicked,
        this, [this](int aid) {
            if (m_window) {
                m_window->setAudioTrack(aid);
            }
        });
    connect(m_window, &ui::player::PlayerWindow::subtitlePicked,
        this, [this](int sid) {
            if (m_window) {
                m_window->setSubtitleTrack(sid);
            }
        });
    connect(m_window, &ui::player::PlayerWindow::speedPicked,
        this, [this](double s) {
            if (m_window) {
                m_window->setSpeed(s);
            }
        });
    connect(m_window, &ui::player::PlayerWindow::userClosedWindow,
        this, [this] { Q_EMIT userClosedWindow(m_ctx); });
    connect(m_window, &QObject::destroyed, this, [this](QObject* obj) {
        if (obj == m_window) {
            m_window = nullptr;
            m_phase = Phase::Idle;
            m_hasActiveSession = false;
            m_paused = false;
            m_position = 0.0;
            m_duration = 0.0;
            Q_EMIT sessionStateChanged();
        }
    });
}

void PlaybackController::play(const QUrl& url,
    const domain::PlaybackContext& ctx)
{
    qCInfo(KINEMA_CONTROLLER).nospace()
        << "PlaybackController::play url="
        << core::redactUrlForLog(url)
        << " title=\"" << ctx.title << "\""
        << " resumeSec="
        << (ctx.resumeSeconds ? *ctx.resumeSeconds : 0);
    // Mark the upcoming mpv `end-file reason="stop"` as a
    // transition artefact rather than a user-meaningful end. mpv's
    // `loadfile` aborts the current file before loading the new
    // one; that abort surfaces as `end-file reason="stop"` and
    // would otherwise look like a stop-of-the-new-file (because
    // `m_ctx` is overwritten immediately below). Without this
    // flag, the series session controller's reactive listeners
    // would clear their prev/next state the moment mpv aborts the
    // previous file, leaving the transport chrome blank after the
    // new episode loads.
    if (m_hasActiveSession && m_window) {
        m_loadfileInFlight = true;
    }
    m_ctx = ctx;
    m_duration = 0.0;
    m_position = 0.0;
    m_paused = false;
    m_pendingResumeSeconds = 0;
    m_skipChapterEnd = -1.0;
    m_chapters.clear();
    ++m_epoch;

    if (!m_window) {
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "Embedded player is not available."),
            6000);
        m_phase = Phase::Idle;
        return;
    }

    if (m_window) {
        m_window->setLoadingVisible(true);
        m_window->hideSkipChapter();
        m_window->hideResumePrompt();
    }

    domain::PlaybackContext loadCtx = ctx;
    if (ctx.resumeSeconds
        && *ctx.resumeSeconds > m_settings.player().resumePromptThresholdSec()) {
        m_pendingResumeSeconds = *ctx.resumeSeconds;
        loadCtx.resumeSeconds.reset();
    }

    m_phase = Phase::Loading;
    m_hasActiveSession = true;
    Q_EMIT activeSessionChanged(m_hasActiveSession);
    Q_EMIT positionChanged(m_position);
    Q_EMIT durationChanged(m_duration);
    Q_EMIT pausedChanged(m_paused);
    Q_EMIT sessionStateChanged();
    Q_EMIT statusMessage(
        i18nc("@info:status", "Loading “%1”…", ctx.title),
        3000);
    // Arm the load watchdog before we hand off to mpv. Disarmed in
    // `onFileLoaded()` / `onEndOfFile()` / `stop()`; on timeout we
    // synthesise an error end-of-file so the queue auto-advances.
    m_loadWatchdog->start();
    m_window->play(url, loadCtx);
}

void PlaybackController::pause()
{
    if (!m_window || !m_hasActiveSession) {
        return;
    }
    m_window->setPaused(true);
}

void PlaybackController::resume()
{
    if (!m_window || !m_hasActiveSession) {
        return;
    }
    m_window->setPaused(false);
}

void PlaybackController::playPause()
{
    if (!m_window || !m_hasActiveSession) {
        return;
    }
    m_window->togglePause();
}

void PlaybackController::stop()
{
    if (!m_window || !m_hasActiveSession) {
        return;
    }
    m_loadWatchdog->stop();
    Q_EMIT userClosedWindow(m_ctx);
    m_phase = Phase::Idle;
    m_hasActiveSession = false;
    m_paused = false;
    m_position = 0.0;
    m_duration = 0.0;
    // `stop()` is the explicit user-close path: any in-flight
    // loadfile that hasn't completed is cancelled, not absorbed.
    m_loadfileInFlight = false;
    m_loadedCtx = {};
    m_window->setLoadingVisible(false);
    Q_EMIT activeSessionChanged(m_hasActiveSession);
    Q_EMIT pausedChanged(m_paused);
    Q_EMIT positionChanged(m_position);
    Q_EMIT durationChanged(m_duration);
    Q_EMIT sessionStateChanged();
    m_window->stopAndHide();
}

void PlaybackController::seekRelativeSeconds(double seconds)
{
    if (!m_window || !m_hasActiveSession) {
        return;
    }
    m_window->seekRelative(seconds);
    Q_EMIT seeked(m_position + seconds);
}

void PlaybackController::seekAbsoluteSeconds(double seconds)
{
    if (!m_window || !m_hasActiveSession || seconds < 0.0) {
        return;
    }
    m_window->seekAbsolute(seconds);
    Q_EMIT seeked(seconds);
}

void PlaybackController::setVolumePercent(double percent)
{
    if (!m_window || !m_hasActiveSession) {
        return;
    }
    m_window->setVolumePercent(percent);
}

void PlaybackController::setPlaybackRate(double factor)
{
    if (!m_window || !m_hasActiveSession) {
        return;
    }
    m_window->setSpeed(factor);
}

void PlaybackController::onFileLoaded()
{
    qCInfo(KINEMA_CONTROLLER) << "PlaybackController: file-loaded";
    m_loadWatchdog->stop();
    // The new file is now the live one. Subsequent `end-file`
    // events refer to *it*, so latch `m_loadedCtx` and clear the
    // loadfile-in-flight flag. Clearing here is also defensive:
    // if mpv ever skips the intermediate `end-file reason="stop"`
    // we still leave a clean slate.
    m_loadedCtx = m_ctx;
    m_loadfileInFlight = false;
    if (m_window) {
        m_window->setLoadingVisible(false);
    }
    if (m_pendingResumeSeconds > 0 && m_window) {
        m_phase = Phase::ShowingResumePrompt;
        m_paused = true;
        m_window->setPaused(true);
        m_window->showResumePrompt(m_pendingResumeSeconds);
    } else {
        m_phase = Phase::Playing;
        m_paused = false;
    }
    Q_EMIT pausedChanged(m_paused);
    Q_EMIT sessionStateChanged();
    Q_EMIT fileLoaded(m_ctx);
}

void PlaybackController::onPlaybackError(const QString& reason)
{
    qCWarning(KINEMA_CONTROLLER).nospace()
        << "PlaybackController: playback error \"" << reason << "\"";
    m_loadWatchdog->stop();
    if (m_window) {
        m_window->setLoadingVisible(false);
    }
    Q_EMIT playbackError(reason, m_ctx);
}

void PlaybackController::onLoadWatchdogTimedOut()
{
    if (m_phase != Phase::Loading) {
        return;
    }
    qCWarning(KINEMA_CONTROLLER)
        << "PlaybackController: load watchdog tripped for"
        << m_ctx.title
        << "after" << m_loadWatchdog->timeout().count() << "ms;"
        << "treating as a playback error so the queue can advance.";
    Q_EMIT statusMessage(
        i18nc("@info:status",
            "Could not start “%1” — the source did not respond.",
            m_ctx.title),
        6000);
    onEndOfFile(QStringLiteral("error"));
}

void PlaybackController::onEndOfFile(const QString& reason)
{
    m_loadWatchdog->stop();

    // Filter the loadfile-induced stop. mpv's `loadfile` aborts
    // the current file before loading the new one; that abort
    // surfaces here as `end-file reason="stop"`. Externally we
    // model `play()`-during-active-session as a continuous
    // session, so swallow the intermediate stop without tearing
    // anything down. The next genuine end-file (eof / error /
    // user-stop) goes through the normal teardown path below.
    if (m_loadfileInFlight && reason == QStringLiteral("stop")) {
        qCDebug(KINEMA_CONTROLLER).nospace()
            << "PlaybackController: absorbing loadfile-induced "
               "end-file=stop for \""
            << m_loadedCtx.title << "\"";
        m_loadfileInFlight = false;
        // Leave m_hasActiveSession, m_phase, m_paused, m_position,
        // m_duration alone — they belong to the upcoming file
        // which `play()` already configured for `Loading`.
        return;
    }
    // Past this point any in-flight loadfile is cancelled (we are
    // tearing down the session). If mpv reports a non-"stop"
    // reason while the flag is set (e.g. an error before the
    // intermediate stop), fall through to the standard teardown.
    m_loadfileInFlight = false;

    ++m_epoch;
    m_pendingResumeSeconds = 0;
    if (m_window) {
        m_window->setLoadingVisible(false);
        m_window->hideSkipChapter();
        m_window->hideResumePrompt();
    }
    m_phase = Phase::Idle;
    m_hasActiveSession = false;
    m_paused = false;
    m_position = 0.0;
    m_duration = 0.0;
    Q_EMIT activeSessionChanged(m_hasActiveSession);
    Q_EMIT pausedChanged(m_paused);
    Q_EMIT positionChanged(m_position);
    Q_EMIT durationChanged(m_duration);
    Q_EMIT sessionStateChanged();
    // Hand out the ctx of the file that actually ended, not the
    // current `m_ctx` (which may have been overwritten by a fresh
    // `play()` that hasn't yet completed its file-load handshake).
    // Fall back to `m_ctx` for first-play edge cases where no
    // `file-loaded` has fired yet (e.g. load watchdog timeout).
    const auto endedCtx = m_loadedCtx.key.isValid()
        ? m_loadedCtx
        : m_ctx;
    m_loadedCtx = {};
    Q_EMIT endOfFile(reason, endedCtx);
}

void PlaybackController::onPositionChanged(double seconds)
{
    m_position = seconds;
    Q_EMIT positionChanged(m_position);
    if (!m_window) {
        return;
    }

    if (m_ctx.key.kind != domain::MediaKind::Series
        || !m_settings.player().skipIntroChapters()) {
        m_skipChapterEnd = -1.0;
        m_window->hideSkipChapter();
        return;
    }

    for (int i = 0; i < m_chapters.size(); ++i) {
        const auto& ch = m_chapters[i];
        const QString title = ch.title.trimmed();
        if (!kSkipRx.match(title).hasMatch()) {
            continue;
        }
        const double start = ch.time;
        double end = -1.0;
        if (i + 1 < m_chapters.size()) {
            end = m_chapters[i + 1].time;
        } else if (m_duration > start) {
            end = m_duration;
        }
        if (end <= start) {
            continue;
        }
        if (seconds >= start && seconds < end) {
            m_skipChapterEnd = end;
            const auto kind = classifySkipChapter(title);
            m_window->showSkipChapter(skipChapterKind(kind),
                skipButtonLabel(kind),
                static_cast<qint64>(start),
                static_cast<qint64>(end));
            return;
        }
    }

    m_skipChapterEnd = -1.0;
    m_window->hideSkipChapter();
}

void PlaybackController::onDurationChanged(double seconds)
{
    m_duration = seconds;
    Q_EMIT durationChanged(m_duration);
    Q_EMIT sessionStateChanged();
}

void PlaybackController::onPausedChanged(bool paused)
{
    if (m_paused == paused) {
        return;
    }
    m_paused = paused;
    Q_EMIT pausedChanged(m_paused);
    Q_EMIT sessionStateChanged();
}

void PlaybackController::onVolumeChanged(double percent)
{
    if (qFuzzyCompare(m_volumePercent + 1.0, percent + 1.0)) {
        return;
    }
    m_volumePercent = percent;
    Q_EMIT sessionStateChanged();
}

void PlaybackController::onSpeedChanged(double factor)
{
    if (qFuzzyCompare(m_playbackRate + 1.0, factor + 1.0)) {
        return;
    }
    m_playbackRate = factor;
    Q_EMIT sessionStateChanged();
}

void PlaybackController::onChaptersChanged(
    const core::chapters::ChapterList& chapters)
{
    m_chapters = chapters;
}

void PlaybackController::onResumeAccepted()
{
    // The controller tracks the pending resume offset itself; the
    // IPC signal from Lua carries no payload, which keeps the
    // wire format minimal and matches the existing state machine.
    const qint64 seconds = m_pendingResumeSeconds;
    m_pendingResumeSeconds = 0;
    if (!m_window) {
        m_phase = Phase::Playing;
        return;
    }
    m_window->hideResumePrompt();
    m_window->seekAbsolute(static_cast<double>(seconds));
    Q_EMIT seeked(static_cast<double>(seconds));
    m_window->setPaused(false);
    m_paused = false;
    m_phase = Phase::Playing;
    Q_EMIT sessionStateChanged();
}

void PlaybackController::onResumeDeclined()
{
    m_pendingResumeSeconds = 0;
    if (!m_window) {
        m_phase = Phase::Playing;
        return;
    }
    m_window->hideResumePrompt();
    m_window->seekAbsolute(0.0);
    Q_EMIT seeked(0.0);
    m_window->setPaused(false);
    m_paused = false;
    m_phase = Phase::Playing;
    Q_EMIT sessionStateChanged();
}

void PlaybackController::onSkipRequested()
{
    if (!m_window || m_skipChapterEnd <= 0.0) {
        return;
    }
    m_window->hideSkipChapter();
    m_window->seekAbsolute(m_skipChapterEnd + 0.25);
    Q_EMIT seeked(m_skipChapterEnd + 0.25);
}

} // namespace kinema::controllers

#endif // KINEMA_HAVE_LIBMPV
