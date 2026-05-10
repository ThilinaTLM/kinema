// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Download.h"
#include "api/Media.h"
#include "api/PlaybackContext.h"

#include <QHash>
#include <QObject>
#include <QPointer>
#include <QSet>
#include <QString>
#include <QUrl>

#include <QCoro/QCoroTask>

#include <map>
#include <memory>
#include <optional>

namespace kinema::api {
class RealDebridClient;
}

namespace kinema::config {
class DownloadSettings;
}

namespace kinema::core {
class DownloadStore;
class HttpClient;
class MediaCache;
}

namespace kinema::torrent {
class TorrentStreamingService;
}

namespace kinema::download {

class AssetSession;
class BackendSelector;
class DownloadBackend;
class HttpAssetSession;
class LocalMediaServer;
class RealDebridResolver;
class TorrentAssetSession;

/// Transient per-asset telemetry merged into download rows. The
/// `DownloadManager` keeps the live values in memory only; only the
/// persisted (cachedBytes / state) fields make it to the store.
struct LiveAssetStats {
    qint64 ratePayloadBps = 0;
    int    peers = 0;
    int    seeds = 0;
    int    etaSeconds = -1;
};

/**
 * Long-lived orchestrator for the unified downloader.
 *
 * Owns: the localhost media server, every active asset session, the
 * RD resolver, the backend selector, and the bridge to the
 * persistent `DownloadStore`. UI code reaches the manager via
 * `controllers::DownloadController`.
 *
 * Mode / disposition contract:
 *   - `prepareForPlayback`: realises (or reuses) a session for the
 *     asset, attaches the player. Mode defaults to OnDemand on a
 *     fresh session; clicking Play on an existing Full session does
 *     not downgrade.
 *   - `enqueueDownload`: realises (or upgrades) a Full + Pinned
 *     session. Clicking Download on an existing OnDemand session
 *     promotes it via `upgradeToFull`.
 *   - `pin/cancel/remove/pause/resume`: per-row lifecycle controls.
 *   - `resumePersisted`: called once at app startup to re-attach
 *     `Full` rows that were active in the previous run.
 */
class DownloadManager : public QObject
{
    Q_OBJECT
public:
    DownloadManager(core::HttpClient& http,
        api::RealDebridClient& rd,
        torrent::TorrentStreamingService& torrentEngine,
        core::DownloadStore& store,
        core::MediaCache& cache,
        const config::DownloadSettings& settings,
        QObject* parent = nullptr);
    ~DownloadManager() override;

    /// Realise or reuse a session for the asset and return a
    /// localhost URL the player can stream from. Defaults to
    /// `OnDemand + Ephemeral` for a new session; never downgrades
    /// an existing Full session.
    QCoro::Task<QUrl> prepareForPlayback(api::Stream stream,
        api::PlaybackContext ctx,
        std::optional<api::DownloadBackendKind> backendOverride = std::nullopt);

    /// Realise or upgrade a Full + Pinned session for the asset.
    /// On an existing OnDemand session: promotes via
    /// `upgradeToFull` instead of duplicating.
    void enqueueDownload(api::Stream stream,
        api::PlaybackContext ctx,
        std::optional<api::DownloadBackendKind> backendOverride = std::nullopt);

    /// Promote an existing OnDemand session to Full + Pinned.
    /// No-op if the asset is already Full or has no active session.
    void upgradeToFull(const QString& assetId);

    /// Track that a media player is currently consuming the asset.
    /// OnDemand sessions surface this in the UI as `Streaming` vs
    /// `Caching`; the persisted state may flip from `Idle` -> `Active`.
    void attachPlayer(const QString& assetId);

    /// Mirror of `attachPlayer`. OnDemand sessions transition to
    /// `Idle` if no other consumer is attached and the row isn't
    /// already complete/failed/cancelled.
    void detachPlayer(const QString& assetId);

    /// User-initiated pause / resume. Updates the persisted state
    /// and the underlying session.
    void pause(const QString& assetId);
    void resume(const QString& assetId);

    /// Re-attach all `Full` rows from the store that are not in a
    /// terminal state. Called once from `MainController` after the
    /// manager is wired up. OnDemand rows are intentionally skipped
    /// because they are consumer-driven.
    void resumePersisted();

    /// Look up the persisted item by playback key, if any.
    std::optional<api::DownloadItem> findForKey(const api::PlaybackKey& key) const;

    /// Toggle a pin marker. Updates both the cache marker and the
    /// store row.
    void pin(const QString& assetId, bool on);

    /// Live transient stats for an asset. Returns nullopt when no
    /// session is active.
    std::optional<LiveAssetStats> liveStatsFor(const QString& assetId) const;

    /// Snapshot of the asset ids that currently have a player
    /// attached. Used by the view-model to compute display chips.
    QSet<QString> attachedPlayerAssetIds() const { return m_attachedPlayers; }

    /// Delete the persisted row plus any cached files. Stops the
    /// session if active.
    void remove(const QString& assetId, bool deleteFiles = true);

    /// Stop work for the given assetId without removing files.
    void cancel(const QString& assetId);

public Q_SLOTS:
    void retry(const QString& assetId);
    void runEviction();

Q_SIGNALS:
    void statusMessage(const QString& text, int timeoutMs = 3000);
    void itemChanged(const QString& assetId);

private:
    /// Ensure a live session exists for `assetId`, recovering it from
    /// the persisted store when needed. Returns nullptr when recovery
    /// is impossible or backend startup fails.
    QCoro::Task<AssetSession*> ensureSessionForAssetId(const QString& assetId);

    /// Realise a session (creating or reusing) and persist the row.
    /// Returns the localhost URL on success; throws on backend failure.
    QCoro::Task<QUrl> openSession(api::AssetRef ref,
        api::Stream stream, api::PlaybackContext ctx,
        api::DownloadMode mode, api::CacheDisposition disposition,
        std::optional<api::DownloadBackendKind> backendOverride);

    /// Fire-and-forget background variant: realises the session,
    /// persists the row, swallows errors into a `Failed` row update.
    QCoro::Task<void> startBackground(api::AssetRef ref,
        api::Stream stream, api::PlaybackContext ctx,
        api::DownloadMode mode, api::CacheDisposition disposition,
        std::optional<api::DownloadBackendKind> backendOverride);

    api::DownloadItem buildItem(const api::AssetRef& ref,
        const api::Stream& s, const api::PlaybackContext& ctx,
        api::DownloadBackendKind backend,
        api::DownloadMode mode,
        api::CacheDisposition disposition) const;

    /// Wire a session's progress signals to the store + itemChanged
    /// broadcast. Used for both backends.
    void installProgressBindings(AssetSession* raw,
        const QString& assetId);

    core::HttpClient& m_http;
    api::RealDebridClient& m_rd;
    torrent::TorrentStreamingService& m_torrentEngine;
    core::DownloadStore& m_store;
    core::MediaCache& m_cache;
    const config::DownloadSettings& m_settings;

    std::unique_ptr<RealDebridResolver> m_resolver;
    std::unique_ptr<LocalMediaServer> m_server;
    std::unique_ptr<BackendSelector> m_selector;

    /// Active sessions keyed by `assetId`. Owned via std::unique_ptr.
    std::map<QString, std::unique_ptr<AssetSession>> m_sessions;

    /// Asset ids currently being opened. Prevents duplicate session
    /// creation when multiple localhost requests race a cold recovery.
    QSet<QString> m_openingAssetIds;

    /// Transient telemetry, populated by session signal handlers.
    /// Cleared when a session is destroyed.
    std::map<QString, LiveAssetStats> m_liveStats;

    /// Asset ids that currently have a media player attached.
    QSet<QString> m_attachedPlayers;
};

} // namespace kinema::download
