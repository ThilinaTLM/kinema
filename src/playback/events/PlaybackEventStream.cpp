// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/events/PlaybackEventStream.h"

namespace kinema::playback::events {

namespace {

void registerMetaTypesOnce()
{
    static const bool registered = [] {
        qRegisterMetaType<kinema::playback::PlaybackSessionId>(
            "kinema::playback::PlaybackSessionId");
        qRegisterMetaType<kinema::playback::PlaybackEndReason>(
            "kinema::playback::PlaybackEndReason");
        qRegisterMetaType<kinema::playback::events::PlaybackEvent>(
            "kinema::playback::events::PlaybackEvent");
        qRegisterMetaType<kinema::domain::MediaFileEntry>(
            "kinema::domain::MediaFileEntry");
        qRegisterMetaType<kinema::domain::EpisodeFileTarget>(
            "kinema::domain::EpisodeFileTarget");
        qRegisterMetaType<kinema::domain::EpisodeAdjacency>(
            "kinema::domain::EpisodeAdjacency");
        return true;
    }();
    Q_UNUSED(registered);
}

template<class... Ts>
struct overloaded : Ts... {
    using Ts::operator()...;
};
template<class... Ts>
overloaded(Ts...) -> overloaded<Ts...>;

} // namespace

PlaybackSessionId sessionIdOf(const PlaybackEvent& e)
{
    return std::visit(
        [](const auto& payload) -> PlaybackSessionId {
            return payload.sessionId;
        },
        e);
}

PlaybackEventStream::PlaybackEventStream(QObject* parent)
    : QObject(parent)
{
    registerMetaTypesOnce();
}

void PlaybackEventStream::publish(const PlaybackEvent& event)
{
    Q_EMIT eventPublished(event);
}

} // namespace kinema::playback::events
