// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/PlaybackController.h"

#ifdef KINEMA_HAVE_LIBMPV

#include "config/AppSettings.h"
#include "controllers/HistoryController.h"
#include "core/HttpClient.h"
#include "core/HttpErrorPresenter.h"
#include "core/Moviehash.h"
#include "core/UrlRedactor.h"
#include "ui/player/PlayerWindow.h"
#include "kinema_debug.h"

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

QString selectedTrackLang(const core::tracks::TrackList& tracks,
    const QString& type)
{
    for (const auto& track : tracks) {
        if (track.type == type && track.selected) {
            return track.lang;
        }
    }
    return {};
}

int trackIdForLanguage(const core::tracks::TrackList& tracks,
    const QString& type, const QString& lang)
{
    for (const auto& track : tracks) {
        if (track.type == type && track.lang == lang && track.id > 0) {
            return track.id;
        }
    }
    return -1;
}

bool hasTrackType(const core::tracks::TrackList& tracks,
    const QString& type)
{
    for (const auto& track : tracks) {
        if (track.type == type) {
            return true;
        }
    }
    return false;
}

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

PlaybackController::PlaybackController(HistoryController& history,
    const config::AppSettings& settings,
    core::HttpClient* http,
    QObject* parent)
    : QObject(parent)
    , m_history(history)
    , m_settings(settings)
    , m_http(http)
{
    connect(&m_loadWatchdog, &PlaybackLoadWatchdog::timedOut,
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
        m_loadWatchdog.stop();
        ++m_streamEpoch;
        Q_EMIT streamCleared();
        m_phase = Phase::Idle;
        m_hasActiveSession = false;
        m_paused = false;
        m_position = 0.0;
        m_duration = 0.0;
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
    connect(m_window, &ui::player::PlayerWindow::trackListChanged,
        this, &PlaybackController::onTrackListChanged);
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
    const api::PlaybackContext& ctx)
{
    m_ctx = ctx;
    m_duration = 0.0;
    m_position = 0.0;
    m_paused = false;
    m_trackMemoryApplied = false;
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

    api::PlaybackContext loadCtx = ctx;
    if (ctx.resumeSeconds
        && *ctx.resumeSeconds > m_settings.player().resumePromptThresholdSec()) {
        m_pendingResumeSeconds = *ctx.resumeSeconds;
        loadCtx.resumeSeconds.reset();
    }

    // Bump stream epoch + kick off best-effort moviehash compute.
    // Any failure is logged at debug level and silently dropped —
    // never user-visible. SubtitleController clears its cached hash
    // on streamCleared() and on a fresh hash arrival.
    ++m_streamEpoch;
    Q_EMIT streamCleared();
    if (m_http) {
        auto t = kickoffMoviehashCompute(url, m_streamEpoch);
        Q_UNUSED(t);
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
    m_loadWatchdog.start();
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
    m_loadWatchdog.stop();
    Q_EMIT userClosedWindow(m_ctx);
    m_phase = Phase::Idle;
    m_hasActiveSession = false;
    m_paused = false;
    m_position = 0.0;
    m_duration = 0.0;
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

QCoro::Task<void> PlaybackController::kickoffMoviehashCompute(QUrl url,
    quint64 epoch)
{
    if (!m_http) {
        co_return;
    }
    constexpr qint64 kBlock = 65536;

    const auto rangeGet = [this, &url](qint64 start, qint64 end)
        -> QCoro::Task<QByteArray> {
        QNetworkRequest req(url);
        req.setRawHeader("Range",
            QByteArrayLiteral("bytes=")
                + QByteArray::number(start)
                + "-"
                + QByteArray::number(end));
        co_return co_await m_http->get(req);
    };

    qint64 size = 0;
    try {
        const auto headers = co_await m_http->head(QNetworkRequest(url));
        for (const auto& h : headers) {
            if (h.first.compare("Content-Length", Qt::CaseInsensitive) == 0) {
                bool ok = false;
                size = h.second.toLongLong(&ok);
                if (!ok) {
                    size = 0;
                }
                break;
            }
        }
    } catch (const std::exception& e) {
        qCDebug(KINEMA) << "moviehash: HEAD failed:" << e.what();
        co_return;
    }
    if (epoch != m_streamEpoch) {
        co_return;
    }
    if (size <= 2 * kBlock) {
        qCDebug(KINEMA) << "moviehash: Content-Length too small or absent";
        co_return;
    }

    QByteArray head;
    QByteArray tail;
    try {
        head = co_await rangeGet(0, kBlock - 1);
    } catch (const std::exception& e) {
        qCDebug(KINEMA) << "moviehash: head Range GET failed:" << e.what();
        co_return;
    }
    if (epoch != m_streamEpoch || head.size() != kBlock) {
        co_return;
    }
    try {
        tail = co_await rangeGet(size - kBlock, size - 1);
    } catch (const std::exception& e) {
        qCDebug(KINEMA) << "moviehash: tail Range GET failed:" << e.what();
        co_return;
    }
    if (epoch != m_streamEpoch || tail.size() != kBlock) {
        co_return;
    }

    const QString hex = core::moviehash::compute(head, tail, size);
    if (hex.isEmpty() || epoch != m_streamEpoch) {
        co_return;
    }
    qCDebug(KINEMA) << "moviehash: computed" << hex << "for"
                    << core::redactUrlForLog(url);
    Q_EMIT moviehashComputed(hex);
}

void PlaybackController::onFileLoaded()
{
    m_loadWatchdog.stop();
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
    m_loadWatchdog.stop();
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
    qCWarning(KINEMA)
        << "PlaybackController: load watchdog tripped for"
        << m_ctx.title
        << "after" << m_loadWatchdog.timeout().count() << "ms;"
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
    m_loadWatchdog.stop();
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
    Q_EMIT endOfFile(reason, m_ctx);
}

void PlaybackController::onPositionChanged(double seconds)
{
    m_position = seconds;
    Q_EMIT positionChanged(m_position);
    if (!m_window) {
        return;
    }

    if (m_ctx.key.kind != api::MediaKind::Series
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

void PlaybackController::onTrackListChanged(
    const core::tracks::TrackList& tracks)
{
    if (m_trackMemoryApplied || tracks.isEmpty() || !m_window
        || !m_ctx.key.isValid()) {
        return;
    }

    const auto stored = m_history.find(m_ctx.key);
    if (!stored.has_value()) {
        m_trackMemoryApplied = true;
        return;
    }

    const QString currentAudio = selectedTrackLang(
        tracks, QStringLiteral("audio"));
    if (!stored->rememberedAudioLang.isEmpty()
        && currentAudio != stored->rememberedAudioLang) {
        const int aid = trackIdForLanguage(tracks,
            QStringLiteral("audio"), stored->rememberedAudioLang);
        if (aid > 0) {
            m_window->setAudioTrack(aid);
        }
    }

    if (stored->rememberedSubtitleLang == QLatin1String("off")) {
        if (hasTrackType(tracks, QStringLiteral("sub"))
            && !selectedTrackLang(tracks, QStringLiteral("sub")).isEmpty()) {
            m_window->setSubtitleTrack(-1);
        }
    } else if (!stored->rememberedSubtitleLang.isEmpty()) {
        const QString currentSub = selectedTrackLang(
            tracks, QStringLiteral("sub"));
        if (currentSub != stored->rememberedSubtitleLang) {
            const int sid = trackIdForLanguage(tracks,
                QStringLiteral("sub"), stored->rememberedSubtitleLang);
            if (sid > 0) {
                m_window->setSubtitleTrack(sid);
            }
        }
    }

    m_trackMemoryApplied = true;
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
