// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Media.h"

#include <QCoro/QCoroTask>

#include <QList>

namespace kinema::playback::ports {

/**
 * Port over `api::IndexerSelector::active()`. Used by
 * `ResumeUseCase` to re-fetch streams for a saved release.
 */
class StreamIndexerPort
{
public:
    virtual ~StreamIndexerPort() = default;

    /// Resolve a fresh list of streams for the given playback key.
    virtual QCoro::Task<QList<domain::Stream>> streamsFor(
        domain::MediaKind kind, const QString& streamId)
        = 0;
};

} // namespace kinema::playback::ports
