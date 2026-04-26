// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/PlaybackController.h"

#ifdef KINEMA_HAVE_LIBMPV

#include "api/CinemetaClient.h"
#include "api/TorrentioClient.h"
#include "config/AppSettings.h"
#include "controllers/HistoryController.h"
#include "core/HttpErrorPresenter.h"
#include "core/NextEpisode.h"
#include "core/StreamFilter.h"
#include "services/StreamActions.h"
#include "ui/player/PlayerWindow.h"

#include <KLocalizedString>

#include <QRegularExpression>
#include <QTimer>

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

int resolutionScore(const api::Stream& s)
{
    const auto& r = s.resolution;
    if (r == QLatin1String("2160p")) return 2160;
    if (r == QLatin1String("1440p")) return 1440;
    if (r == QLatin1String("1080p")) return 1080;
    if (r == QLatin1String("720p")) return 720;
    if (r == QLatin1String("480p")) return 480;
    if (r == QLatin1String("360p")) return 360;
    return 0;
}

QString displayTitleForEpisode(const QString& seriesTitle,
    const api::Episode& ep)
{
    QString display = seriesTitle;
    if (!display.isEmpty()) {
        display += QStringLiteral(" — ");
    }
    display += QStringLiteral("S%1E%2")
                   .arg(ep.season, 2, 10, QLatin1Char('0'))
                   .arg(ep.number, 2, 10, QLatin1Char('0'));
    if (!ep.title.isEmpty()) {
        display += QStringLiteral(" — ") + ep.title;
    }
    return display;
}

std::optional<api::Stream> pickBestStream(QList<api::Stream> streams,
    const config::AppSettings& settings)
{
    core::stream_filter::ClientFilters filters;
    filters.cachedOnly = settings.realDebrid().configured()
        && settings.torrentio().cachedOnly();
    filters.keywordBlocklist = settings.filter().keywordBlocklist();

    auto visible = core::stream_filter::apply(streams, filters);
    visible.erase(std::remove_if(visible.begin(), visible.end(),
                     [](const api::Stream& s) {
                         return s.directUrl.isEmpty();
                     }),
        visible.end());
    if (visible.isEmpty()) {
        return std::nullopt;
    }

    const auto sort = settings.torrentio().defaultSort();
    std::stable_sort(visible.begin(), visible.end(),
        [sort](const api::Stream& lhs, const api::Stream& rhs) {
            switch (sort) {
            case core::torrentio::SortMode::Size:
                if (lhs.sizeBytes.value_or(0) != rhs.sizeBytes.value_or(0)) {
                    return lhs.sizeBytes.value_or(0) > rhs.sizeBytes.value_or(0);
                }
                return lhs.seeders.value_or(0) > rhs.seeders.value_or(0);
            case core::torrentio::SortMode::QualitySize:
                if (resolutionScore(lhs) != resolutionScore(rhs)) {
                    return resolutionScore(lhs) > resolutionScore(rhs);
                }
                if (lhs.sizeBytes.value_or(0) != rhs.sizeBytes.value_or(0)) {
                    return lhs.sizeBytes.value_or(0) > rhs.sizeBytes.value_or(0);
                }
                return lhs.seeders.value_or(0) > rhs.seeders.value_or(0);
            case core::torrentio::SortMode::Seeders:
            default:
                if (lhs.seeders.value_or(0) != rhs.seeders.value_or(0)) {
                    return lhs.seeders.value_or(0) > rhs.seeders.value_or(0);
                }
                if (resolutionScore(lhs) != resolutionScore(rhs)) {
                    return resolutionScore(lhs) > resolutionScore(rhs);
                }
                return lhs.sizeBytes.value_or(0) > rhs.sizeBytes.value_or(0);
            }
        });

    return visible.first();
}

} // namespace

PlaybackController::PlaybackController(api::CinemetaClient& cinemeta,
    api::TorrentioClient& torrentio,
    services::StreamActions& actions,
    HistoryController& history,
    const config::AppSettings& settings,
    const QString& rdTokenRef,
    QObject* parent)
    : QObject(parent)
    , m_cinemeta(cinemeta)
    , m_torrentio(torrentio)
    , m_actions(actions)
    , m_history(history)
    , m_settings(settings)
    , m_rdToken(rdTokenRef)
{
    m_nextEpisodeTimer = new QTimer(this);
    m_nextEpisodeTimer->setInterval(1000);
    connect(m_nextEpisodeTimer, &QTimer::timeout,
        this, &PlaybackController::onNextEpisodeCountdownTick);
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
    connect(m_window, &ui::player::PlayerWindow::nextEpisodeAccepted,
        this, &PlaybackController::onNextEpisodeAccepted);
    connect(m_window, &ui::player::PlayerWindow::nextEpisodeCancelled,
        this, &PlaybackController::onNextEpisodeCancelled);
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
    m_nextEpisodeTriggered = false;
    m_trackMemoryApplied = false;
    m_pendingResumeSeconds = 0;
    m_skipChapterEnd = -1.0;
    m_nextEpisodeCountdownRemaining = 0;
    m_nextEpisodeCtx = {};
    m_nextEpisodeStream = {};
    m_chapters.clear();
    if (m_nextEpisodeTimer) {
        m_nextEpisodeTimer->stop();
    }
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
        m_window->hideNextEpisodeBanner();
        m_window->hideSkipChapter();
        m_window->hideResumePrompt();
    }

    api::PlaybackContext loadCtx = ctx;
    if (ctx.resumeSeconds
        && *ctx.resumeSeconds > m_settings.player().resumePromptThresholdSec()) {
        m_pendingResumeSeconds = *ctx.resumeSeconds;
        loadCtx.resumeSeconds.reset();
    }

    m_phase = Phase::Loading;
    Q_EMIT statusMessage(
        i18nc("@info:status", "Loading “%1”…", ctx.title),
        3000);
    m_window->play(url, loadCtx);
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
    if (m_nextEpisodeTimer) {
        m_nextEpisodeTimer->stop();
    }
    if (m_window) {
        m_window->hideNextEpisodeBanner();
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

    if (m_ctx.key.kind == api::MediaKind::Series
        && m_settings.player().autoplayNextEpisode()
        && !m_nextEpisodeTriggered && m_duration > 0.0) {
        const double remaining = m_duration - seconds;
        if (seconds >= m_duration * 0.95 || remaining <= 60.0) {
            m_nextEpisodeTriggered = true;
            auto task = playNextEpisodeTask(m_ctx.key);
            Q_UNUSED(task);
        }
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

void PlaybackController::onNextEpisodeAccepted()
{
    if (m_nextEpisodeTimer) {
        m_nextEpisodeTimer->stop();
    }
    if (m_window) {
        m_window->hideNextEpisodeBanner();
    }
    if (!m_nextEpisodeStream.directUrl.isEmpty() && m_nextEpisodeCtx.key.isValid()) {
        m_actions.play(m_nextEpisodeStream, m_nextEpisodeCtx);
    }
}

void PlaybackController::onNextEpisodeCancelled()
{
    if (m_nextEpisodeTimer) {
        m_nextEpisodeTimer->stop();
    }
    if (m_window) {
        m_window->hideNextEpisodeBanner();
    }
}

void PlaybackController::onNextEpisodeCountdownTick()
{
    if (m_nextEpisodeCountdownRemaining <= 0) {
        onNextEpisodeAccepted();
        return;
    }
    --m_nextEpisodeCountdownRemaining;
    if (m_nextEpisodeCountdownRemaining <= 0) {
        onNextEpisodeAccepted();
        return;
    }
    if (m_window) {
        m_window->updateNextEpisodeCountdown(
            m_nextEpisodeCountdownRemaining);
    }
}

QCoro::Task<void> PlaybackController::playNextEpisodeTask(api::PlaybackKey from)
{
    const auto myEpoch = m_epoch;

    try {
        auto sd = co_await m_cinemeta.seriesMeta(from.imdbId);
        if (myEpoch != m_epoch) {
            co_return;
        }

        const auto nextKey = core::series::nextEpisodeKey(from, sd.episodes);
        if (!nextKey.has_value()) {
            co_return;
        }

        auto nextEpIt = std::find_if(sd.episodes.cbegin(), sd.episodes.cend(),
            [nextKey](const api::Episode& ep) {
                return ep.season == nextKey->season.value_or(-1)
                    && ep.number == nextKey->episode.value_or(-1);
            });
        if (nextEpIt == sd.episodes.cend()) {
            co_return;
        }

        auto opts = m_settings.torrentioOptions();
        opts.realDebridToken = m_rdToken;
        auto streams = co_await m_torrentio.streams(
            nextKey->kind, nextKey->storageKey(), opts);
        if (myEpoch != m_epoch) {
            co_return;
        }

        const auto best = pickBestStream(streams, m_settings);
        if (!best.has_value()) {
            co_return;
        }

        api::PlaybackContext nextCtx;
        nextCtx.key = *nextKey;
        nextCtx.seriesTitle = sd.meta.summary.title;
        nextCtx.episodeTitle = nextEpIt->title;
        nextCtx.title = displayTitleForEpisode(sd.meta.summary.title, *nextEpIt);
        nextCtx.poster = sd.meta.summary.poster.isValid()
            ? sd.meta.summary.poster
            : nextEpIt->thumbnail;

        m_nextEpisodeCtx = nextCtx;
        m_nextEpisodeStream = *best;
        m_nextEpisodeCountdownRemaining = qMax(0,
            m_settings.player().autoNextCountdownSec());
        if (!m_window) {
            co_return;
        }
        m_phase = Phase::NearEnd;
        m_window->showNextEpisodeBanner(
            m_nextEpisodeCtx, m_nextEpisodeCountdownRemaining);
        if (m_nextEpisodeCountdownRemaining <= 0) {
            onNextEpisodeAccepted();
            co_return;
        }
        m_nextEpisodeTimer->start();
    } catch (const std::exception& e) {
        if (myEpoch != m_epoch) {
            co_return;
        }
        Q_EMIT statusMessage(core::describeError(e, "next episode"), 6000);
    }
}

} // namespace kinema::controllers

#endif // KINEMA_HAVE_LIBMPV
