// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/session/PlaybackSessionManager.h"

#include "core/io/HttpErrorPresenter.h"
#include "domain/Download.h"
#include "playback/adapters/ExternalPlayerAdapter.h"
#include "playback/events/PlaybackEventStream.h"
#include "playback/ports/PlayerPort.h"
#include "playback/resume/ResumeUseCase.h"
#include "playback/series/SeriesSessionService.h"
#include "playback/session/PlaybackSession.h"
#include "playback/transfer/TransferUseCase.h"

#ifdef KINEMA_HAVE_LIBMPV
#include "playback/adapters/EmbeddedMpvPlayerAdapter.h"
#endif

#include <KLocalizedString>

#include <stdexcept>
#include <variant>

namespace kinema::playback::session {

PlaybackSessionManager::PlaybackSessionManager(
    events::PlaybackEventStream& eventStream,
    transfer::TransferUseCase& transfers,
    adapters::EmbeddedMpvPlayerAdapter* embeddedAdapter,
    adapters::ExternalPlayerAdapter* externalAdapter,
    QObject* parent)
    : QObject(parent)
    , m_eventStream(eventStream)
    , m_transfers(&transfers)
    , m_embeddedAdapter(embeddedAdapter)
    , m_externalAdapter(externalAdapter)
{
    connect(m_transfers, &transfer::TransferUseCase::statusMessage,
        this, &PlaybackSessionManager::statusMessage);
    connect(&m_eventStream, &events::PlaybackEventStream::eventPublished,
        this, &PlaybackSessionManager::onEvent);
#ifdef KINEMA_HAVE_LIBMPV
    if (m_embeddedAdapter) {
        connect(m_embeddedAdapter,
            &adapters::EmbeddedMpvPlayerAdapter::visibilityChanged,
            this, &PlaybackSessionManager::visibilityChanged);
        connect(m_embeddedAdapter,
            &adapters::EmbeddedMpvPlayerAdapter::statusMessage,
            this, &PlaybackSessionManager::statusMessage);
    }
#endif
}

PlaybackSessionManager::PlaybackSessionManager(
    events::PlaybackEventStream& eventStream,
    adapters::EmbeddedMpvPlayerAdapter* embeddedAdapter,
    adapters::ExternalPlayerAdapter* externalAdapter,
    QObject* parent)
    : QObject(parent)
    , m_eventStream(eventStream)
    , m_embeddedAdapter(embeddedAdapter)
    , m_externalAdapter(externalAdapter)
{
    connect(&m_eventStream, &events::PlaybackEventStream::eventPublished,
        this, &PlaybackSessionManager::onEvent);
#ifdef KINEMA_HAVE_LIBMPV
    if (m_embeddedAdapter) {
        connect(m_embeddedAdapter,
            &adapters::EmbeddedMpvPlayerAdapter::visibilityChanged,
            this, &PlaybackSessionManager::visibilityChanged);
        connect(m_embeddedAdapter,
            &adapters::EmbeddedMpvPlayerAdapter::statusMessage,
            this, &PlaybackSessionManager::statusMessage);
    }
#endif
}

PlaybackSessionManager::~PlaybackSessionManager() = default;

void PlaybackSessionManager::setResumeUseCase(
    resume::ResumeUseCase* resume) noexcept
{
    m_resume = resume;
}

void PlaybackSessionManager::setSeriesSessionService(
    series::SeriesSessionService* series) noexcept
{
    m_series = series;
    if (m_series) {
        connect(m_series, SIGNAL(navigationChanged()),
            this, SIGNAL(navigationChanged()), Qt::UniqueConnection);
    }
}

PlaybackSessionId PlaybackSessionManager::activeSessionId() const noexcept
{
    return m_session ? m_session->id() : PlaybackSessionId();
}

void PlaybackSessionManager::supersedeActiveSession()
{
    if (m_session && !m_session->isTerminal()) {
        m_session->markReplacedByNewSource();
    }
    if (m_session) {
        m_session.reset();
        Q_EMIT activeSessionChanged(false);
    }
}

domain::PlaybackContext PlaybackSessionManager::effectiveContext(
    const domain::Stream& stream,
    const domain::PlaybackContext& ctxIn) const
{
    domain::PlaybackContext ctx = ctxIn;
    ctx.streamRef = domain::HistoryStreamRef::fromStream(stream);
    if (ctx.title.isEmpty()) {
        ctx.title = stream.releaseName.isEmpty()
            ? stream.qualityLabel
            : stream.releaseName;
    }
    if (m_resume) {
        ctx.resumeSeconds = m_resume->resumeSecondsFor(ctx.key);
    }
    return ctx;
}

void PlaybackSessionManager::stampAdapters(
    const domain::PlaybackContext& ctx)
{
    if (!m_session) {
        return;
    }
    const auto id = m_session->id();
    if (m_externalAdapter) {
        m_externalAdapter->setActiveSession(id, ctx);
    }
#ifdef KINEMA_HAVE_LIBMPV
    if (m_embeddedAdapter) {
        m_embeddedAdapter->setActiveSession(id, ctx);
    }
#endif
}

void PlaybackSessionManager::play(const domain::Stream& stream,
    const domain::PlaybackContext& ctx)
{
    startPlay(stream, ctx, std::nullopt);
}

void PlaybackSessionManager::playWithBackend(const domain::Stream& stream,
    const domain::PlaybackContext& ctx,
    domain::DownloadBackendKind backend)
{
    startPlay(stream, ctx, backend);
}

void PlaybackSessionManager::startPlay(const domain::Stream& stream,
    const domain::PlaybackContext& ctxIn,
    std::optional<domain::DownloadBackendKind> backendOverride)
{
    if (stream.directUrl.isEmpty() && stream.infoHash.isEmpty()) {
        Q_EMIT statusMessage(i18nc("@info:status",
            "This release has no playable URL or magnet."), 5000);
        return;
    }

    supersedeActiveSession();
    const auto ctx = effectiveContext(stream, ctxIn);
    m_session = std::make_unique<PlaybackSession>(m_eventStream);
    m_session->start(stream, ctx, backendOverride);
    stampAdapters(ctx);
    Q_EMIT activeSessionChanged(true);
    Q_EMIT playbackStateChanged();

    auto task = playTask(m_session->id(), stream, ctx, backendOverride);
    Q_UNUSED(task);
}

QCoro::Task<void> PlaybackSessionManager::playTask(
    PlaybackSessionId sessionId,
    domain::Stream stream,
    domain::PlaybackContext ctx,
    std::optional<domain::DownloadBackendKind> backendOverride)
{
    try {
        QUrl url;
        QString assetId;
        if (!stream.infoHash.isEmpty()) {
            const auto ref = domain::assetRefFor(stream, ctx);
            assetId = domain::assetIdFor(ref);
            if (m_session && m_session->id() == sessionId) {
                m_eventStream.publish(events::SourceResolving { sessionId });
                m_session->markSourceResolved(ref);
                m_eventStream.publish(events::TransferOpening {
                    sessionId, assetId });
            }
            if (!m_transfers) {
                throw std::runtime_error(i18nc("@info:status",
                    "Torrent streaming is not available in this build.")
                        .toStdString());
            }
            url = co_await m_transfers->ensurePlayable(sessionId,
                stream, ctx, backendOverride);
            if (!m_session || m_session->id() != sessionId) {
                co_return;
            }
            m_eventStream.publish(events::TransferReady { sessionId, assetId });
        } else {
            url = stream.directUrl;
        }

        if (!url.isValid() || url.isEmpty()) {
            throw std::runtime_error(i18nc("@info:status",
                "No playable URL available for this item.").toStdString());
        }

        if (!m_session || m_session->id() != sessionId) {
            co_return;
        }
        m_session->markPlayableUrl(assetId, url);
        stampAdapters(ctx);

        // ExternalPlayerAdapter delegates to PlayerLauncher, which still
        // honours the user's preferred player. If Embedded is preferred,
        // PlayerLauncher emits embeddedRequested and ShellViewModel opens
        // the window; the embedded adapter has already been stamped above.
        if (m_externalAdapter && m_externalAdapter->isAvailable()) {
            m_externalAdapter->play(url, ctx, ctx.resumeSeconds);
        }
#ifdef KINEMA_HAVE_LIBMPV
        else if (m_embeddedAdapter && m_embeddedAdapter->isAvailable()) {
            m_embeddedAdapter->play(url, ctx, ctx.resumeSeconds);
        }
#endif
        else if (m_externalAdapter) {
            // Let PlayerLauncher produce the canonical failure/status text.
            m_externalAdapter->play(url, ctx, ctx.resumeSeconds);
        } else {
            throw std::runtime_error(i18nc("@info:status",
                "No supported media player found. Install mpv or VLC, then try again.")
                    .toStdString());
        }
    } catch (const std::exception& e) {
        if (!m_session || m_session->id() != sessionId) {
            co_return;
        }
        const QString reason = core::describeError(e, "playback");
        m_session->markFailed(reason);
        Q_EMIT statusMessage(reason, 6000);
    }
}

void PlaybackSessionManager::download(const domain::Stream& stream,
    const domain::PlaybackContext& ctx)
{
    if (!m_transfers) {
        Q_EMIT statusMessage(i18nc("@info:status",
            "Downloads are not available in this build."), 5000);
        return;
    }
    m_transfers->saveOffline(stream, effectiveContext(stream, ctx));
}

void PlaybackSessionManager::downloadWithBackend(
    const domain::Stream& stream,
    const domain::PlaybackContext& ctx,
    domain::DownloadBackendKind backend)
{
    if (!m_transfers) {
        Q_EMIT statusMessage(i18nc("@info:status",
            "Downloads are not available in this build."), 5000);
        return;
    }
    m_transfers->saveOffline(stream, effectiveContext(stream, ctx), backend);
}

ports::PlayerPort* PlaybackSessionManager::commandPlayer() const noexcept
{
#ifdef KINEMA_HAVE_LIBMPV
    if (m_embeddedAdapter && m_embeddedAdapter->snapshot().active) {
        return m_embeddedAdapter;
    }
#endif
    return m_externalAdapter;
}

void PlaybackSessionManager::pause()
{
    if (auto* p = commandPlayer()) p->pause();
}

void PlaybackSessionManager::resume()
{
    if (auto* p = commandPlayer()) p->resume();
}

void PlaybackSessionManager::playPause()
{
    if (auto* p = commandPlayer()) p->togglePause();
}

void PlaybackSessionManager::stop()
{
    if (auto* p = commandPlayer()) {
        p->stop();
        return;
    }
    if (m_session) {
        m_session->stopByUser();
    }
}

void PlaybackSessionManager::seekRelativeSeconds(double seconds)
{
    if (auto* p = commandPlayer()) p->seekRelative(seconds);
}

void PlaybackSessionManager::seekAbsoluteSeconds(double seconds)
{
    if (auto* p = commandPlayer()) p->seekAbsolute(seconds);
}

void PlaybackSessionManager::setVolumePercent(double percent)
{
    if (auto* p = commandPlayer()) p->setVolumePercent(percent);
}

void PlaybackSessionManager::setPlaybackRate(double factor)
{
    if (auto* p = commandPlayer()) p->setPlaybackRate(factor);
}

void PlaybackSessionManager::selectAudioTrack(int id)
{
    if (auto* p = commandPlayer()) p->selectAudioTrack(id);
}

void PlaybackSessionManager::selectSubtitleTrack(int id)
{
    if (auto* p = commandPlayer()) p->selectSubtitleTrack(id);
}

void PlaybackSessionManager::attachSubtitle(const QString& localPath,
    const QString& language)
{
    if (localPath.isEmpty()) {
        return;
    }
    if (auto* p = commandPlayer()) {
        p->attachSubtitleFile(localPath, language);
    }
}

void PlaybackSessionManager::playNextEpisode()
{
    if (m_series) {
        m_series->playNextEpisode();
    }
}

void PlaybackSessionManager::playPreviousEpisode()
{
    if (m_series) {
        m_series->playPreviousEpisode();
    }
}

void PlaybackSessionManager::onEvent(const events::PlaybackEvent& event)
{
    const auto sid = events::sessionIdOf(event);
    if (sid.isNull() || sid != activeSessionId()) {
        return;
    }
    std::visit([this](const auto& payload) {
        using T = std::decay_t<decltype(payload)>;
        if constexpr (std::is_same_v<T, events::PlayerLoaded>
            || std::is_same_v<T, events::PlayerLoading>) {
            Q_EMIT playbackStateChanged();
        } else if constexpr (std::is_same_v<T, events::PlaybackStateChanged>) {
            Q_EMIT playbackStateChanged();
            Q_EMIT pausedChanged(payload.paused);
        } else if constexpr (std::is_same_v<T, events::PositionTicked>) {
            Q_EMIT positionChanged(payload.seconds);
        } else if constexpr (std::is_same_v<T, events::DurationChanged>) {
            Q_EMIT durationChanged(payload.seconds);
        } else if constexpr (std::is_same_v<T, events::PlaybackEnded>
            || std::is_same_v<T, events::PlaybackFailed>) {
            Q_EMIT playbackStateChanged();
            Q_EMIT activeSessionChanged(false);
        }
    }, event);
}

} // namespace kinema::playback::session
