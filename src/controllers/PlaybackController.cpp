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

QString skipButtonLabel(const QString& chapterTitle)
{
    const QString t = chapterTitle.trimmed();
    if (t.contains(QRegularExpression(
            QStringLiteral("^(credits|end credits)\\b"),
            QRegularExpression::CaseInsensitiveOption))) {
        return i18nc("@action:button", "Skip credits");
    }
    if (t.contains(QRegularExpression(
            QStringLiteral("^(outro|ending)\\b"),
            QRegularExpression::CaseInsensitiveOption))) {
        return i18nc("@action:button", "Skip outro");
    }
    return i18nc("@action:button", "Skip intro");
}

/// Return the stable kind string the Lua chrome uses as the auto-
/// skip toggle key. Matches the regex families in `skipButtonLabel`.
QString skipChapterKind(const QString& chapterTitle)
{
    const QString t = chapterTitle.trimmed();
    if (t.contains(QRegularExpression(
            QStringLiteral("^(credits|end credits)\\b"),
            QRegularExpression::CaseInsensitiveOption))) {
        return QStringLiteral("credits");
    }
    if (t.contains(QRegularExpression(
            QStringLiteral("^(outro|ending)\\b"),
            QRegularExpression::CaseInsensitiveOption))) {
        return QStringLiteral("outro");
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
        ++m_streamEpoch;
        Q_EMIT streamCleared();
        m_phase = Phase::Idle;
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
    connect(m_window, &QObject::destroyed, this, [this](QObject* obj) {
        if (obj == m_window) {
            m_window = nullptr;
            m_phase = Phase::Idle;
        }
    });
}

void PlaybackController::play(const QUrl& url,
    const api::PlaybackContext& ctx)
{
    m_ctx = ctx;
    m_duration = 0.0;
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
    Q_EMIT statusMessage(
        i18nc("@info:status", "Loading “%1”…", ctx.title),
        3000);
    m_window->play(url, loadCtx);
}

QCoro::Task<void> PlaybackController::kickoffMoviehashCompute(QUrl url,
    quint64 epoch)
{
    if (!m_http) {
        co_return;
    }
    constexpr qint64 kBlock = 65536;

    qint64 size = 0;
    try {
        QNetworkRequest headReq(url);
        const auto headers = co_await m_http->head(headReq);
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

    QByteArray head, tail;
    try {
        QNetworkRequest headReq(url);
        headReq.setRawHeader("Range",
            QByteArrayLiteral("bytes=0-")
                + QByteArray::number(kBlock - 1));
        head = co_await m_http->get(headReq);
    } catch (const std::exception& e) {
        qCDebug(KINEMA) << "moviehash: head Range GET failed:" << e.what();
        co_return;
    }
    if (epoch != m_streamEpoch || head.size() != kBlock) {
        co_return;
    }
    try {
        QNetworkRequest tailReq(url);
        const auto start = size - kBlock;
        tailReq.setRawHeader("Range",
            QByteArrayLiteral("bytes=")
                + QByteArray::number(start)
                + "-"
                + QByteArray::number(size - 1));
        tail = co_await m_http->get(tailReq);
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
    if (m_pendingResumeSeconds > 0 && m_window) {
        m_phase = Phase::ShowingResumePrompt;
        m_window->setPaused(true);
        m_window->showResumePrompt(m_pendingResumeSeconds);
    } else {
        m_phase = Phase::Playing;
    }
    Q_EMIT fileLoaded(m_ctx);
}

void PlaybackController::onPlaybackError(const QString& reason)
{
    Q_EMIT playbackError(reason, m_ctx);
}

void PlaybackController::onEndOfFile(const QString& reason)
{
    ++m_epoch;
    m_pendingResumeSeconds = 0;
    if (m_window) {
        m_window->hideSkipChapter();
        m_window->hideResumePrompt();
    }
    m_phase = Phase::Idle;
    Q_EMIT endOfFile(reason, m_ctx);
}

void PlaybackController::onPositionChanged(double seconds)
{
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
            // Cast to qint64 truncates towards zero; fine for
            // timeline-band rendering which is in whole seconds.
            m_window->showSkipChapter(skipChapterKind(title),
                skipButtonLabel(title),
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
    m_window->setPaused(false);
    m_phase = Phase::Playing;
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
    m_window->setPaused(false);
    m_phase = Phase::Playing;
}

void PlaybackController::onSkipRequested()
{
    if (!m_window || m_skipChapterEnd <= 0.0) {
        return;
    }
    m_window->hideSkipChapter();
    m_window->seekAbsolute(m_skipChapterEnd + 0.25);
}

} // namespace kinema::controllers

#endif // KINEMA_HAVE_LIBMPV
