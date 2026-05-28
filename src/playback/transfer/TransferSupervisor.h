// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "playback/events/PlaybackEvent.h"
#include "playback/transfer/LiveAssetStats.h"

#include <QObject>
#include <QString>

#include <functional>
#include <map>
#include <optional>

namespace kinema::playback::events {
class PlaybackEventStream;
}

namespace kinema::playback::ports {
class DownloadRepository;
}

namespace kinema::playback::transfer {

class SessionRegistry;
class TransferSession;

/**
 * Owns the progress / state projection from live transfer sessions
 * into the persistent `DownloadRepository` and the typed
 * `PlaybackEventStream`.
 *
 * The supervisor subscribes to `SessionRegistry::sessionRegistered`
 * and wires each new `TransferSession`'s progress signals into:
 *
 *   - the repository (`updateCachedBytes`, `updateState`,
 *     `setLastError`) — mirroring the legacy
 *     `DownloadManager::installProgressBindings` semantics; and
 *   - the playback event stream — publishing typed
 *     `TransferProgressed` / `TransferStatsChanged` /
 *     `TransferCompleted` / `TransferFailed` events.
 *
 * The mapping from `assetId` to the active `PlaybackSessionId`
 * (used in the typed events for cross-correlation with the
 * matching playback session) is supplied via
 * `setSessionIdResolver`. The default resolver returns a null
 * `PlaybackSessionId`, which is correct for background downloads
 * that have no attached player.
 */
class TransferSupervisor : public QObject
{
    Q_OBJECT
public:
    using SessionIdResolver
        = std::function<PlaybackSessionId(const QString& assetId)>;

    TransferSupervisor(SessionRegistry& registry,
        ports::DownloadRepository& repo,
        events::PlaybackEventStream& events,
        QObject* parent = nullptr);
    ~TransferSupervisor() override;

    TransferSupervisor(const TransferSupervisor&) = delete;
    TransferSupervisor& operator=(const TransferSupervisor&) = delete;

    /// Override the assetId → PlaybackSessionId resolver. Used by
    /// `PlaybackSession` / `PlaybackSessionManager` to make
    /// foreground playback transfers cross-correlate with their
    /// owning session in the event stream.
    void setSessionIdResolver(SessionIdResolver fn);

    /// Read-only access to the latest transient telemetry sourced
    /// from `TransferSession::liveStatsChanged`. Returns nullopt
    /// when no stats have been observed for `assetId` (e.g. before
    /// the first tick, or after the session was unregistered).
    /// View-models join this onto the persistent `DownloadItem`
    /// rows when populating per-row rate / peers / ETA cells.
    std::optional<LiveAssetStats> liveStatsFor(
        const QString& assetId) const;

Q_SIGNALS:
    /// Forwards `DownloadRepository` mutations so legacy QObject
    /// consumers (the soon-to-be-rewired `DownloadController`) can
    /// keep emitting `itemChanged(assetId)` without subscribing to
    /// the event stream directly.
    void itemChanged(const QString& assetId);

private:
    void onSessionRegistered(const QString& assetId);
    void onSessionRemoved(const QString& assetId);
    void bind(TransferSession* session);
    void onCachedBytes(TransferSession* session, qint64 bytes);
    void onCompleted(TransferSession* session);
    void onFailed(TransferSession* session, const QString& reason);
    void onLiveStats(TransferSession* session, qint64 rate, int peers,
        int seeds, int eta);

    PlaybackSessionId resolveSessionId(const QString& assetId) const;

    SessionRegistry& m_registry;
    ports::DownloadRepository& m_repo;
    events::PlaybackEventStream& m_events;
    SessionIdResolver m_resolver;

    /// Live telemetry snapshot keyed by `assetId`. Populated from
    /// the session's `liveStatsChanged` signal; cleared on
    /// `completed` / `failed` / explicit unregister.
    std::map<QString, LiveAssetStats> m_liveStats;
};

} // namespace kinema::playback::transfer
