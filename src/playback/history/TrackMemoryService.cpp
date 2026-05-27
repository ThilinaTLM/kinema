// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/history/TrackMemoryService.h"

#include "playback/events/PlaybackEventStream.h"
#include "playback/history/HistoryQueryService.h"
#include "playback/ports/PlayerPort.h"

#include <variant>

namespace kinema::playback::history {

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

} // namespace

TrackMemoryService::TrackMemoryService(events::PlaybackEventStream& events,
    HistoryQueryService& history,
    ports::PlayerPort* player,
    QObject* parent)
    : QObject(parent)
    , m_events(events)
    , m_history(history)
    , m_player(player)
{
    connect(&m_events, &events::PlaybackEventStream::eventPublished,
        this, &TrackMemoryService::onEvent);
}

TrackMemoryService::~TrackMemoryService() = default;

void TrackMemoryService::onEvent(const events::PlaybackEvent& event)
{
    std::visit([this](const auto& payload) {
        using T = std::decay_t<decltype(payload)>;
        if constexpr (std::is_same_v<T, events::PlaybackRequested>) {
            onPlaybackRequested(payload);
        } else if constexpr (std::is_same_v<T, events::TrackListChanged>) {
            onTrackListChanged(payload);
        }
    }, event);
}

void TrackMemoryService::onPlaybackRequested(
    const events::PlaybackRequested& e)
{
    m_sessionId = e.sessionId;
    m_ctx = e.ctx;
    m_appliedThisSession = false;
}

void TrackMemoryService::onTrackListChanged(
    const events::TrackListChanged& e)
{
    if (!m_player || m_appliedThisSession) {
        return;
    }
    if (e.sessionId != m_sessionId) {
        return;
    }
    if (e.tracks.isEmpty() || !m_ctx.key.isValid()) {
        return;
    }

    const auto stored = m_history.find(m_ctx.key);
    if (!stored.has_value()) {
        // Nothing remembered - first watch. Latch so a follow-up
        // TrackListChanged (e.g. caused by a track-change we
        // initiated) doesn't re-evaluate.
        m_appliedThisSession = true;
        return;
    }

    const auto& tracks = e.tracks;

    const QString currentAudio = selectedTrackLang(
        tracks, QStringLiteral("audio"));
    if (!stored->rememberedAudioLang.isEmpty()
        && currentAudio != stored->rememberedAudioLang) {
        const int aid = trackIdForLanguage(tracks,
            QStringLiteral("audio"), stored->rememberedAudioLang);
        if (aid > 0) {
            m_player->selectAudioTrack(aid);
        }
    }

    if (stored->rememberedSubtitleLang == QLatin1String("off")) {
        // User previously turned subtitles off for this title;
        // mpv may have re-enabled the default sub track on load,
        // so honour the preference explicitly.
        if (hasTrackType(tracks, QStringLiteral("sub"))
            && !selectedTrackLang(tracks, QStringLiteral("sub"))
                    .isEmpty()) {
            m_player->selectSubtitleTrack(-1);
        }
    } else if (!stored->rememberedSubtitleLang.isEmpty()) {
        const QString currentSub = selectedTrackLang(
            tracks, QStringLiteral("sub"));
        if (currentSub != stored->rememberedSubtitleLang) {
            const int sid = trackIdForLanguage(tracks,
                QStringLiteral("sub"), stored->rememberedSubtitleLang);
            if (sid > 0) {
                m_player->selectSubtitleTrack(sid);
            }
        }
    }

    m_appliedThisSession = true;
}

} // namespace kinema::playback::history
