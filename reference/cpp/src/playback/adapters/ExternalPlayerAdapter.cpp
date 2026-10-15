// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/adapters/ExternalPlayerAdapter.h"

#include "core/mpv/Player.h"
#include "core/mpv/PlayerLauncher.h"
#include "playback/events/PlaybackEventStream.h"

namespace kinema::playback::adapters {

ExternalPlayerAdapter::ExternalPlayerAdapter(
    core::PlayerLauncher& launcher,
    events::PlaybackEventStream& eventStream,
    QObject* parent)
    : QObject(parent)
    , m_launcher(launcher)
    , m_eventStream(eventStream)
{
    connect(&m_launcher, &core::PlayerLauncher::launched,
        this, &ExternalPlayerAdapter::onLaunched);
    connect(&m_launcher, &core::PlayerLauncher::launchFailed,
        this, &ExternalPlayerAdapter::onLaunchFailed);
}

ExternalPlayerAdapter::~ExternalPlayerAdapter() = default;

void ExternalPlayerAdapter::setActiveSession(PlaybackSessionId sessionId,
    const domain::PlaybackContext& ctx)
{
    m_sessionId = sessionId;
    m_ctx = ctx;
    m_sessionActive = !sessionId.isNull();
}

bool ExternalPlayerAdapter::isAvailable() const
{
    return m_launcher.preferredPlayerAvailable();
}

void ExternalPlayerAdapter::play(const QUrl& url,
    const domain::PlaybackContext& ctx,
    std::optional<qint64> /*resumeSeconds*/)
{
    // External players don't accept reliable resume from us; the
    // session-level resume policy continues to seed
    // ctx.resumeSeconds for the embedded adapter only.
    m_launcher.play(url, ctx);
}

void ExternalPlayerAdapter::pause() {}
void ExternalPlayerAdapter::resume() {}
void ExternalPlayerAdapter::togglePause() {}

void ExternalPlayerAdapter::stop()
{
    // We can't actually stop a detached external process, but we
    // can close out our own session bookkeeping so the projector
    // and history layers see a terminal event.
    if (!m_sessionActive) {
        return;
    }
    m_eventStream.publish(events::PlaybackEnded {
        m_sessionId,
        PlaybackEndReason::UserStop,
        m_ctx,
    });
    m_sessionActive = false;
}

void ExternalPlayerAdapter::seekRelative(double) {}
void ExternalPlayerAdapter::seekAbsolute(double) {}
void ExternalPlayerAdapter::setVolumePercent(double) {}
void ExternalPlayerAdapter::setPlaybackRate(double) {}
void ExternalPlayerAdapter::selectAudioTrack(int) {}
void ExternalPlayerAdapter::selectSubtitleTrack(int) {}

bool ExternalPlayerAdapter::attachSubtitleFile(const QString&, const QString&)
{
    return false;
}

ports::PlayerSnapshot ExternalPlayerAdapter::snapshot() const
{
    ports::PlayerSnapshot s;
    s.active = m_sessionActive;
    return s;
}

void ExternalPlayerAdapter::onLaunched(core::player::Kind /*kind*/,
    const QString& /*title*/)
{
    if (!m_sessionActive) {
        return;
    }
    m_eventStream.publish(events::PlayerLoaded { m_sessionId });
}

void ExternalPlayerAdapter::onLaunchFailed(core::player::Kind /*kind*/,
    const QString& reason)
{
    if (!m_sessionActive) {
        return;
    }
    m_eventStream.publish(events::PlaybackFailed {
        m_sessionId,
        reason,
        m_ctx,
    });
    m_sessionActive = false;
}

} // namespace kinema::playback::adapters
