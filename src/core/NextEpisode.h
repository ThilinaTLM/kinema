// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"
#include "api/PlaybackContext.h"

#include <optional>

namespace kinema::core::series {

/// Given the current episode key and a fetched Cinemeta episode list,
/// return the next regular episode in series order. Specials (season
/// 0) are skipped. Returns nullopt for invalid / non-series keys or
/// when the current episode is the finale.
std::optional<api::PlaybackKey> nextEpisodeKey(
    const api::PlaybackKey& current,
    const QList<api::Episode>& episodes);

} // namespace kinema::core::series
