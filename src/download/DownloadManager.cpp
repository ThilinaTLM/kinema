// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "download/DownloadManager.h"

#include "api/RealDebridClient.h"
#include "config/DownloadSettings.h"
#include "core/DownloadStore.h"
#include "core/HttpClient.h"
#include "core/MediaCache.h"
#include "download/AssetSession.h"
#include "download/BackendSelector.h"
#include "download/DownloadBackend.h"
#include "download/HttpAssetSession.h"
#include "download/LocalMediaServer.h"
#include "download/RealDebridBackend.h"
#include "download/RealDebridResolver.h"
#include "download/TorrentAssetSession.h"
#include "download/TorrentBackend.h"
#include "kinema_log_download.h"
#include "torrent/TorrentStreamingService.h"

#include <KLocalizedString>

#include <QCoro/QCoroSignal>
#include <QTimer>

#include <stdexcept>

namespace kinema::download {

namespace {

const char* modeName(api::DownloadMode m)
{
    return m == api::DownloadMode::Full ? "Full" : "OnDemand";
}

const char* dispositionName(api::CacheDisposition d)
{
    return d == api::CacheDisposition::Pinned ? "Pinned" : "Ephemeral";
}

const char* backendName(api::DownloadBackendKind k)
{
    return k == api::DownloadBackendKind::RealDebridHttp
        ? "RealDebridHttp"
        : "Torrent";
}

QCoro::Task<void> sleepMs(int ms)
{
    QTimer timer;
    timer.setSingleShot(true);
    timer.start(ms);
    co_await qCoro(&timer, &QTimer::timeout);
    co_return;
}

} // namespace

DownloadManager::DownloadManager(core::HttpClient& http,
    api::RealDebridClient& rd,
    torrent::TorrentStreamingService& torrentEngine,
    core::DownloadStore& store,
    core::MediaCache& cache,
    const config::DownloadSettings& settings,
    QObject* parent)
    : QObject(parent)
    , m_http(http)
    , m_rd(rd)
    , m_torrentEngine(torrentEngine)
    , m_store(store)
    , m_cache(cache)
    , m_settings(settings)
    , m_resolver(std::make_unique<RealDebridResolver>(rd, this))
    , m_server(std::make_unique<LocalMediaServer>(this))
    , m_selector(std::make_unique<BackendSelector>())
{
    if (!m_server->listen()) {
        qCWarning(KINEMA_DOWNLOAD)
            << "DownloadManager: could not bind localhost server";
    } else {
        qCInfo(KINEMA_DOWNLOAD)
            << "DownloadManager ready; localhost server listening";
    }
    m_server->setSessionResolver(
        [this](const QString& assetId) -> QCoro::Task<AssetSession*> {
            co_return co_await ensureSessionForAssetId(assetId);
        });

    // Order matters: the selector tries backends in priority order
    // when no override is given. Real-Debrid first since it's the
    // faster transport when available.
    m_selector->registerBackend(std::make_unique<RealDebridBackend>(
        m_http, m_rd, *m_resolver, m_cache, m_settings));
    m_selector->registerBackend(std::make_unique<TorrentBackend>(
        m_torrentEngine, m_cache));
}

DownloadManager::~DownloadManager() = default;

api::DownloadItem DownloadManager::buildItem(const api::AssetRef& ref,
    const api::Stream& s, const api::PlaybackContext& ctx,
    api::DownloadBackendKind backend,
    api::DownloadMode mode,
    api::CacheDisposition disposition) const
{
    api::DownloadItem it;
    it.assetId = api::assetIdFor(ref);
    it.backendKind = backend;
    it.state = api::DownloadState::Resolving;
    it.mode = mode;
    it.disposition = disposition;
    it.key = ctx.key;
    it.title = ctx.title.isEmpty() ? s.releaseName : ctx.title;
    it.seriesTitle = ctx.seriesTitle;
    it.episodeTitle = ctx.episodeTitle;
    it.poster = ctx.poster;
    it.infoHash = ref.infoHash;
    it.releaseName = ref.releaseName;
    it.fileIndex = ref.fileIndex;
    it.fileNameHint = ref.fileNameHint;
    it.qualityLabel = ref.qualityLabel;
    it.resolution = ref.resolution;
    it.provider = ref.provider;
    it.expectedSizeBytes = ref.sizeBytes;
    it.localDir = m_cache.assetDir(it.assetId).absolutePath();
    return it;
}

std::optional<api::DownloadItem> DownloadManager::findForKey(
    const api::PlaybackKey& key) const
{
    return m_store.findForKey(key);
}

QCoro::Task<QUrl> DownloadManager::prepareForPlayback(api::Stream stream,
    api::PlaybackContext ctx,
    std::optional<api::DownloadBackendKind> backendOverride)
{
    const auto ref = api::assetRefFor(stream, ctx);
    if (!ref.isValid()) {
        // Direct-URL-only candidate without an info hash. We still
        // play it via the player (no localhost server for it) so we
        // throw and let the caller fall back to launching the URL
        // directly.
        throw std::runtime_error(i18nc("@info:status",
            "This stream has no playable info hash.").toStdString());
    }
    const auto assetId = api::assetIdFor(ref);

    // Preserve a previously persisted row's mode + disposition.
    // A second click on Play does not downgrade an existing Full
    // session.
    const auto existing = m_store.find(assetId);
    const auto mode = existing
        ? existing->mode
        : api::DownloadMode::OnDemand;
    const auto disposition = existing
        ? existing->disposition
        : api::CacheDisposition::Ephemeral;

    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "prepareForPlayback assetId=" << assetId
        << " mode=" << modeName(mode)
        << " disposition=" << dispositionName(disposition);

    const auto url = co_await openSession(ref, std::move(stream),
        std::move(ctx), mode, disposition, backendOverride);
    attachPlayer(assetId);
    co_return url;
}

void DownloadManager::enqueueDownload(api::Stream stream,
    api::PlaybackContext ctx,
    std::optional<api::DownloadBackendKind> backendOverride)
{
    const auto ref = api::assetRefFor(stream, ctx);
    if (!ref.isValid()) {
        Q_EMIT statusMessage(i18nc("@info:status",
            "Cannot download: stream has no playable info hash."), 5000);
        qCWarning(KINEMA_DOWNLOAD)
            << "enqueueDownload rejected: invalid AssetRef (no info hash)";
        return;
    }
    const auto assetId = api::assetIdFor(ref);

    // Already-active session: upgrade in place rather than spawning
    // a duplicate.
    if (m_sessions.find(assetId) != m_sessions.end()) {
        upgradeToFull(assetId);
        return;
    }

    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "enqueueDownload assetId=" << assetId
        << " mode=Full disposition=Pinned";

    auto task = startBackground(ref, std::move(stream), std::move(ctx),
        api::DownloadMode::Full, api::CacheDisposition::Pinned,
        backendOverride);
    Q_UNUSED(task);
}

void DownloadManager::upgradeToFull(const QString& assetId)
{
    auto sit = m_sessions.find(assetId);
    if (sit == m_sessions.end()) {
        qCInfo(KINEMA_DOWNLOAD)
            << "upgradeToFull: no active session for" << assetId;
        return;
    }
    auto& session = *sit->second;
    if (session.mode() == api::DownloadMode::Full) {
        // Still ensure pin marker is set in case the user pinned
        // by hand earlier and we lost track.
        m_cache.setPinned(assetId, true);
        m_store.setDisposition(assetId, api::CacheDisposition::Pinned);
        return;
    }

    auto* backend = m_selector->find(
        m_store.find(assetId)
            ? m_store.find(assetId)->backendKind
            : api::DownloadBackendKind::Torrent);
    if (!backend) {
        qCWarning(KINEMA_DOWNLOAD)
            << "upgradeToFull: no backend registered for" << assetId;
        return;
    }
    backend->changeMode(session, api::DownloadMode::Full);

    m_store.updateMode(assetId, api::DownloadMode::Full);
    m_cache.setPinned(assetId, true);
    m_store.setDisposition(assetId, api::CacheDisposition::Pinned);
    Q_EMIT itemChanged(assetId);

    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "upgradeToFull assetId=" << assetId
        << " (OnDemand -> Full + Pinned)";
}

void DownloadManager::attachPlayer(const QString& assetId)
{
    if (assetId.isEmpty()) {
        return;
    }
    if (m_attachedPlayers.contains(assetId)) {
        return;
    }
    m_attachedPlayers.insert(assetId);
    // OnDemand rows previously parked at Idle return to Active when
    // a consumer reattaches. Full rows ignore the flag because they
    // never sit at Idle.
    if (auto row = m_store.find(assetId);
        row && row->state == api::DownloadState::Idle) {
        m_store.updateState(assetId, api::DownloadState::Active);
    }
    Q_EMIT itemChanged(assetId);
}

void DownloadManager::detachPlayer(const QString& assetId)
{
    if (assetId.isEmpty()) {
        return;
    }
    if (!m_attachedPlayers.remove(assetId)) {
        return;
    }
    if (auto row = m_store.find(assetId)) {
        if (row->mode == api::DownloadMode::OnDemand
            && row->state == api::DownloadState::Active) {
            // Park at Idle so the UI can show that we're alive but
            // not currently fetching for a consumer. The torrent
            // engine's own idle-stop timer will eventually quiesce
            // the libtorrent handle (governed by
            // TorrentStreamingSettings::idleStopMinutes).
            m_store.updateState(assetId, api::DownloadState::Idle);
        }
    }
    Q_EMIT itemChanged(assetId);
}

void DownloadManager::pause(const QString& assetId)
{
    auto sit = m_sessions.find(assetId);
    if (sit != m_sessions.end()) {
        sit->second->pause();
    }
    m_store.updateState(assetId, api::DownloadState::Paused);
    Q_EMIT itemChanged(assetId);
    qCInfo(KINEMA_DOWNLOAD) << "pause assetId=" << assetId;
}

void DownloadManager::resume(const QString& assetId)
{
    auto sit = m_sessions.find(assetId);
    if (sit != m_sessions.end()) {
        sit->second->resume();
        m_store.updateState(assetId, api::DownloadState::Active);
    } else {
        // No live session — kick a retry.
        retry(assetId);
        return;
    }
    Q_EMIT itemChanged(assetId);
    qCInfo(KINEMA_DOWNLOAD) << "resume assetId=" << assetId;
}

void DownloadManager::resumePersisted()
{
    using S = api::DownloadState;
    using M = api::DownloadMode;

    int resumed = 0;
    for (const auto& row : m_store.loadAll()) {
        if (row.mode != M::Full) {
            continue;
        }
        if (row.state == S::Completed
            || row.state == S::Failed
            || row.state == S::Cancelled) {
            continue;
        }
        if (m_sessions.find(row.assetId) != m_sessions.end()) {
            continue;
        }

        // Normalise stale mid-flight states from the previous run.
        m_store.updateState(row.assetId, S::Queued);

        auto args = core::DownloadStore::synthesiseStartArgs(row);
        auto task = startBackground(args.ref, args.stream, args.ctx,
            M::Full, row.disposition, row.backendKind);
        Q_UNUSED(task);
        ++resumed;
    }
    qCInfo(KINEMA_DOWNLOAD)
        << "resumePersisted: kicked" << resumed << "Full sessions";
}

QCoro::Task<AssetSession*> DownloadManager::ensureSessionForAssetId(
    const QString& assetId)
{
    if (assetId.isEmpty()) {
        co_return nullptr;
    }
    if (auto it = m_sessions.find(assetId); it != m_sessions.end()) {
        it->second->touch();
        co_return it->second.get();
    }

    const auto row = m_store.find(assetId);
    if (!row) {
        co_return nullptr;
    }

    auto args = core::DownloadStore::synthesiseStartArgs(*row);
    try {
        co_await openSession(args.ref, args.stream, args.ctx,
            row->mode, row->disposition, row->backendKind);
    } catch (const std::exception& e) {
        qCWarning(KINEMA_DOWNLOAD).nospace()
            << "ensureSessionForAssetId failed assetId=" << assetId
            << " reason=\"" << QString::fromUtf8(e.what()) << "\"";
        co_return nullptr;
    }

    auto it = m_sessions.find(assetId);
    co_return it == m_sessions.end() ? nullptr : it->second.get();
}

QCoro::Task<QUrl> DownloadManager::openSession(api::AssetRef ref,
    api::Stream stream, api::PlaybackContext ctx,
    api::DownloadMode mode, api::CacheDisposition disposition,
    std::optional<api::DownloadBackendKind> backendOverride)
{
    const auto assetId = api::assetIdFor(ref);

    // Reuse an active session if one exists for this exact asset.
    if (auto it = m_sessions.find(assetId); it != m_sessions.end()) {
        auto* existing = it->second.get();
        existing->touch();
        co_return m_server->urlFor(existing);
    }

    // Series-pack episode navigation can request a different file
    // inside the same info hash. The torrent backend only serves
    // one selected file per live hash session, so supersede any
    // older same-hash asset sessions before opening the new one.
    if (!ref.infoHash.isEmpty()) {
        QStringList superseded;
        for (const auto& [otherAssetId, session] : m_sessions) {
            if (otherAssetId == assetId) {
                continue;
            }
            auto* torrentSession
                = qobject_cast<TorrentAssetSession*>(session.get());
            if (!torrentSession
                || torrentSession->infoHash() != ref.infoHash) {
                continue;
            }
            superseded.append(otherAssetId);
        }
        for (const auto& otherAssetId : superseded) {
            if (auto it = m_sessions.find(otherAssetId);
                it != m_sessions.end()) {
                m_server->unregisterSession(it->second.get());
                m_sessions.erase(it);
            }
            m_liveStats.erase(otherAssetId);
            m_attachedPlayers.remove(otherAssetId);
            if (const auto row = m_store.find(otherAssetId);
                row && row->mode == api::DownloadMode::OnDemand
                && row->state == api::DownloadState::Active) {
                m_store.updateState(otherAssetId,
                    api::DownloadState::Idle);
                Q_EMIT itemChanged(otherAssetId);
            }
            m_cache.markInactive(otherAssetId);
        }
    }

    while (m_openingAssetIds.contains(assetId)) {
        if (auto it = m_sessions.find(assetId); it != m_sessions.end()) {
            auto* existing = it->second.get();
            existing->touch();
            co_return m_server->urlFor(existing);
        }
        co_await sleepMs(25);
    }
    if (auto it = m_sessions.find(assetId); it != m_sessions.end()) {
        auto* existing = it->second.get();
        existing->touch();
        co_return m_server->urlFor(existing);
    }

    m_openingAssetIds.insert(assetId);

    try {
        auto* backend = m_selector->select(stream, backendOverride);
        if (!backend) {
            throw std::runtime_error(i18nc("@info:status",
                "No download backend can serve this stream.")
                                         .toStdString());
        }

        // Persist the row immediately so the UI shows a Resolving entry
        // while the backend opens its session.
        auto item = buildItem(ref, stream, ctx, backend->kind(), mode,
            disposition);
        m_store.upsert(item);
        if (disposition == api::CacheDisposition::Pinned) {
            m_cache.setPinned(assetId, true);
        }

        Q_EMIT statusMessage(mode == api::DownloadMode::Full
                ? i18nc("@info:status", "Starting download\u2026")
                : i18nc("@info:status", "Buffering stream\u2026"),
            0);

        auto session = co_await backend->open(ref, stream, ctx, mode);
        auto* raw = session.get();
        installProgressBindings(raw, assetId);
        m_server->registerSession(raw);
        m_sessions.emplace(assetId, std::move(session));

        item.state = api::DownloadState::Active;
        item.expectedSizeBytes = raw->fileSize() > 0
            ? std::optional<qint64>(raw->fileSize())
            : item.expectedSizeBytes;

        // Seed the persisted progress from whatever the session
        // discovered on disk (HttpAssetSession::loadChunkMap, or a
        // torrent resume populated by the engine). Without this,
        // re-opening a session over an already-cached payload
        // leaves cached_size_bytes at 0 forever — fetchChunk only
        // emits cachedBytesChanged on chunks it actually downloads,
        // so a fully-cached play stays at 0% on the Downloads row.
        const qint64 initialCached = raw->cachedBytes();
        if (initialCached >= 0) {
            item.cachedSizeBytes = initialCached;
            const qint64 expected = raw->fileSize();
            if (expected > 0 && initialCached >= expected) {
                item.complete = true;
                item.state = api::DownloadState::Completed;
            }
        }
        m_store.upsert(item);
        Q_EMIT itemChanged(assetId);

        const QUrl url = m_server->urlFor(raw);
        m_openingAssetIds.remove(assetId);
        co_return url;
    } catch (...) {
        m_openingAssetIds.remove(assetId);
        throw;
    }
}

QCoro::Task<void> DownloadManager::startBackground(api::AssetRef ref,
    api::Stream stream, api::PlaybackContext ctx,
    api::DownloadMode mode, api::CacheDisposition disposition,
    std::optional<api::DownloadBackendKind> backendOverride)
{
    const auto assetId = api::assetIdFor(ref);
    try {
        co_await openSession(std::move(ref), std::move(stream),
            std::move(ctx), mode, disposition, backendOverride);
    } catch (const std::exception& e) {
        const auto reason = QString::fromUtf8(e.what());
        m_store.updateState(assetId, api::DownloadState::Failed, reason);
        Q_EMIT itemChanged(assetId);
        qCWarning(KINEMA_DOWNLOAD).nospace()
            << "startBackground failed assetId=" << assetId
            << " reason=\"" << reason << "\"";
    }
}

void DownloadManager::installProgressBindings(AssetSession* raw,
    const QString& assetId)
{
    connect(raw, &AssetSession::cachedBytesChanged, this,
        [this, assetId](qint64 bytes) {
            // Promote Queued/Resolving/Idle rows to Active the moment
            // we see real bytes flowing.
            const auto found = m_store.find(assetId);
            const bool wasIdle = found
                && (found->state == api::DownloadState::Queued
                    || found->state == api::DownloadState::Resolving
                    || found->state == api::DownloadState::Idle);
            m_store.updateProgress(assetId, bytes, false);
            if (wasIdle) {
                m_store.updateState(assetId,
                    api::DownloadState::Active);
            }
            Q_EMIT itemChanged(assetId);
        });
    connect(raw, &AssetSession::completed, this, [this, assetId] {
        const auto found = m_store.find(assetId);
        m_store.updateProgress(assetId,
            found ? found->expectedSizeBytes.value_or(0) : 0, true);
        m_store.updateState(assetId, api::DownloadState::Completed);
        m_liveStats.erase(assetId);
        Q_EMIT itemChanged(assetId);
        qCInfo(KINEMA_DOWNLOAD).nospace()
            << "asset completed assetId=" << assetId;
    });
    connect(raw, &AssetSession::failed, this,
        [this, assetId](const QString& reason) {
            m_store.updateState(assetId, api::DownloadState::Failed,
                reason);
            m_liveStats.erase(assetId);
            Q_EMIT itemChanged(assetId);
            qCWarning(KINEMA_DOWNLOAD).nospace()
                << "asset failed assetId=" << assetId
                << " reason=\"" << reason << "\"";
        });
    connect(raw, &AssetSession::liveStatsChanged, this,
        [this, assetId](qint64 rate, int peers, int seeds, int eta) {
            auto& s = m_liveStats[assetId];
            s.ratePayloadBps = rate;
            s.peers = peers;
            s.seeds = seeds;
            s.etaSeconds = eta;
            Q_EMIT itemChanged(assetId);
        });
}

std::optional<LiveAssetStats> DownloadManager::liveStatsFor(
    const QString& assetId) const
{
    const auto it = m_liveStats.find(assetId);
    if (it == m_liveStats.end()) {
        return std::nullopt;
    }
    return it->second;
}

void DownloadManager::pin(const QString& assetId, bool on)
{
    m_cache.setPinned(assetId, on);
    m_store.setDisposition(assetId,
        on ? api::CacheDisposition::Pinned
           : api::CacheDisposition::Ephemeral);
    // Mirror the pin onto the live torrent session so the engine's
    // idle-stop respects "Save offline" intent. Other backends are
    // no-ops here.
    if (const auto found = m_store.find(assetId);
        found && !found->infoHash.isEmpty()) {
        m_torrentEngine.setKeepAlive(found->infoHash, on);
    }
    qCInfo(KINEMA_DOWNLOAD)
        << "pin assetId=" << assetId << "-> on=" << on;
    Q_EMIT itemChanged(assetId);
}

void DownloadManager::cancel(const QString& assetId)
{
    qCInfo(KINEMA_DOWNLOAD) << "cancel assetId=" << assetId;
    const auto row = m_store.find(assetId);
    if (row && !row->infoHash.isEmpty()) {
        m_torrentEngine.stopInfoHash(row->infoHash);
    }
    if (auto it = m_sessions.find(assetId); it != m_sessions.end()) {
        m_server->unregisterSession(it->second.get());
        m_sessions.erase(it);
    }
    m_liveStats.erase(assetId);
    m_attachedPlayers.remove(assetId);
    m_cache.markInactive(assetId);
    m_store.updateState(assetId, api::DownloadState::Cancelled);
    Q_EMIT itemChanged(assetId);
}

void DownloadManager::remove(const QString& assetId, bool deleteFiles)
{
    qCInfo(KINEMA_DOWNLOAD) << "remove assetId=" << assetId
                            << "deleteFiles=" << deleteFiles;
    const auto row = m_store.find(assetId);
    if (row && !row->infoHash.isEmpty()) {
        m_torrentEngine.stopInfoHash(row->infoHash);
    }
    if (auto it = m_sessions.find(assetId); it != m_sessions.end()) {
        m_server->unregisterSession(it->second.get());
        m_sessions.erase(it);
    }
    m_liveStats.erase(assetId);
    m_attachedPlayers.remove(assetId);
    m_cache.markInactive(assetId);
    m_cache.setPinned(assetId, false);
    if (deleteFiles) {
        m_cache.removeAsset(assetId);
    }
    m_store.remove(assetId);
}

void DownloadManager::retry(const QString& assetId)
{
    const auto found = m_store.find(assetId);
    if (!found) {
        qCInfo(KINEMA_DOWNLOAD)
            << "retry: no persisted row for assetId=" << assetId;
        return;
    }
    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "retry assetId=" << assetId
        << " backend=" << backendName(found->backendKind)
        << " mode=" << modeName(found->mode);

    m_store.updateState(assetId, api::DownloadState::Queued);
    Q_EMIT itemChanged(assetId);

    // Drop any half-broken session before restarting.
    if (auto it = m_sessions.find(assetId); it != m_sessions.end()) {
        m_server->unregisterSession(it->second.get());
        m_sessions.erase(it);
    }
    m_liveStats.erase(assetId);

    auto args = core::DownloadStore::synthesiseStartArgs(*found);
    auto task = startBackground(args.ref, args.stream, args.ctx,
        found->mode, found->disposition, found->backendKind);
    Q_UNUSED(task);
}

void DownloadManager::runEviction()
{
    m_cache.enforceBudget();
}

} // namespace kinema::download
