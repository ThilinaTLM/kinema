// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/NextEpisode.h"

#include <algorithm>

namespace kinema::core::series {

std::optional<api::PlaybackKey> nextEpisodeKey(
    const api::PlaybackKey& current,
    const QList<api::Episode>& episodes)
{
    if (current.kind != api::MediaKind::Series || current.imdbId.isEmpty()
        || !current.season || !current.episode) {
        return std::nullopt;
    }

    QList<api::Episode> ordered = episodes;
    std::sort(ordered.begin(), ordered.end(),
        [](const api::Episode& lhs, const api::Episode& rhs) {
            if (lhs.season != rhs.season) {
                return lhs.season < rhs.season;
            }
            return lhs.number < rhs.number;
        });

    const int curSeason = *current.season;
    const int curEpisode = *current.episode;

    for (const auto& ep : ordered) {
        if (ep.season <= 0 || ep.number <= 0) {
            continue;
        }
        if (ep.season < curSeason) {
            continue;
        }
        if (ep.season == curSeason && ep.number <= curEpisode) {
            continue;
        }

        api::PlaybackKey next;
        next.kind = api::MediaKind::Series;
        next.imdbId = current.imdbId;
        next.season = ep.season;
        next.episode = ep.number;
        return next;
    }

    return std::nullopt;
}

} // namespace kinema::core::series
