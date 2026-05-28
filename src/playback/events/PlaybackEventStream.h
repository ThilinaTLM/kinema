// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "playback/events/PlaybackEvent.h"

#include <QObject>

namespace kinema::playback::events {

/**
 * Typed event bus shared by the playback subsystem. Components
 * publish typed `PlaybackEvent` values; projections subscribe with
 * `Qt::QueuedConnection` or direct connections depending on their
 * thread.
 *
 * Not a global service locator. Only the playback subsystem and its
 * tests/projections may subscribe. Discover/search/library
 * view-models must not subscribe directly.
 */
class PlaybackEventStream : public QObject
{
    Q_OBJECT
public:
    explicit PlaybackEventStream(QObject* parent = nullptr);

    /// Publish one event. Synchronous Q_EMIT under the hood; queued
    /// subscribers see it on the next event loop tick.
    void publish(const PlaybackEvent& event);

Q_SIGNALS:
    void eventPublished(const kinema::playback::events::PlaybackEvent& event);
};

} // namespace kinema::playback::events
