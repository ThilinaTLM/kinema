// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "download/DownloadManager.h"

#include "api/RealDebridClient.h"
#include "config/DownloadSettings.h"
#include "core/DownloadStore.h"
#include "core/HttpClient.h"
#include "core/HttpErrorPresenter.h"
#include "core/MediaCache.h"
#include "download/HttpAssetSession.h"
#include "download/LocalMediaServer.h"
#include "download/RealDebridResolver.h"
#include "download/TorrentAssetSession.h"
#include "kinema_log_download.h"
#include "torrent/TorrentStreamingService.h"

#include <KLocalizedString>

#include <stdexcept>

namespace kinema::download {

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
{
    if (!m_server->listen()) {
        qCWarning(KINEMA_DOWNLOAD)
            << "DownloadManager: could not bind localhost server";
    } else {
        qCInfo(KINEMA_DOWNLOAD)
            << "DownloadManager ready; localhost server listening";
    }
}

DownloadManager::~DownloadManager() = default;

api::DownloadBackendKind DownloadManager::chooseBackend(
    const api::Stream& s) const
{
    // RD path requires a token + a hint that RD has the content
    // (`rdCached` from the availability enrichment, or RD-style
    // direct URL on the row).
    const bool rdConfigured = !m_rd.token().isEmpty();
    const bool rdEligible = rdConfigured
        && (s.rdCached || !s.directUrl.isEmpty());
    const auto backend = rdEligible
        ? api::DownloadBackendKind::RealDebridHttp
        : api::DownloadBackendKind::Torrent;
    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "chooseBackend: rdConfigured=" << rdConfigured
        << " rdCached=" << s.rdCached
        << " hasDirectUrl=" << !s.directUrl.isEmpty()
        << " hash=" << s.infoHash.left(8)
        << " -> "
        << (backend == api::DownloadBackendKind::RealDebridHttp
                ? "RealDebridHttp"
                : "Torrent");
    return backend;
}

api::DownloadItem DownloadManager::buildItem(const api::AssetRef& ref,
    const api::Stream& s, const api::PlaybackContext& ctx,
    api::DownloadBackendKind backend,
    api::CacheDisposition disposition) const
{
    api::DownloadItem it;
    it.assetId = api::assetIdFor(ref);
    it.backendKind = backend;
    it.state = api::DownloadState::Preparing;
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

QCoro::Task<QUrl> DownloadManager::prepareForPlayback(
    api::Stream stream, api::PlaybackContext ctx)
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

    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "prepareForPlayback assetId=" << api::assetIdFor(ref);
    const auto backend = chooseBackend(stream);
    if (backend == api::DownloadBackendKind::RealDebridHttp) {
        co_return co_await prepareHttp(ref, stream, ctx);
    }
    co_return co_await prepareTorrent(ref, stream, ctx);
}

QCoro::Task<QUrl> DownloadManager::prepareTorrent(api::AssetRef ref,
    api::Stream stream, api::PlaybackContext ctx)
{
    const auto assetId = api::assetIdFor(ref);
    m_cache.markActive(assetId);

    // Preserve a previously persisted disposition (e.g. Pinned)
    // when Play later touches an asset that was already enqueued.
    const auto existing = m_store.find(assetId);
    const auto disposition = existing
        ? existing->disposition
        : api::CacheDisposition::Ephemeral;
    auto item = buildItem(ref, stream, ctx,
        api::DownloadBackendKind::Torrent, disposition);
    m_store.upsert(item);

    // Reuse an active session if we have one for this asset.
    {
        const auto it = m_sessions.find(assetId);
        if (it != m_sessions.end()) {
            auto* existing = it->second.get();
            existing->touch();
            co_return m_server->urlFor(existing);
        }
    }

    Q_EMIT statusMessage(i18nc("@info:status",
        "Buffering torrent stream\u2026"), 0);

    const auto prepared = co_await m_torrentEngine.prepareSession(
        stream, ctx, torrent::PrepareMode::Streaming);
    auto session = std::make_unique<TorrentAssetSession>(m_torrentEngine,
        assetId, prepared.token, prepared.fileName, prepared.fileSize,
        prepared.infoHash, this);
    auto* raw = session.get();
    installTorrentProgressBindings(raw, assetId);
    m_server->registerSession(raw);
    m_sessions.emplace(assetId, std::move(session));
    if (disposition == api::CacheDisposition::Pinned) {
        m_torrentEngine.setKeepAlive(prepared.infoHash, true);
    }

    item.state = api::DownloadState::Streaming;
    item.expectedSizeBytes = prepared.fileSize;
    m_store.upsert(item);
    Q_EMIT itemChanged(assetId);

    co_return m_server->urlFor(raw);
}

QCoro::Task<QUrl> DownloadManager::prepareHttp(api::AssetRef ref,
    api::Stream stream, api::PlaybackContext ctx)
{
    Q_UNUSED(stream);
    const auto assetId = api::assetIdFor(ref);
    m_cache.markActive(assetId);

    const auto existing = m_store.find(assetId);
    const auto disposition = existing
        ? existing->disposition
        : api::CacheDisposition::Ephemeral;
    auto item = buildItem(ref, stream, ctx,
        api::DownloadBackendKind::RealDebridHttp, disposition);
    m_store.upsert(item);

    {
        const auto it = m_sessions.find(assetId);
        if (it != m_sessions.end()) {
            auto* existing = it->second.get();
            existing->touch();
            co_return m_server->urlFor(existing);
        }
    }

    auto session = std::make_unique<HttpAssetSession>(
        m_http, *m_resolver, m_settings, ref, assetId,
        m_cache.assetDir(assetId).absolutePath(), this);
    auto* raw = session.get();
    installHttpProgressBindings(raw, assetId);

    Q_EMIT statusMessage(i18nc("@info:status",
        "Resolving Real-Debrid link\u2026"), 0);

    try {
        co_await raw->ensureResolved();
    } catch (const std::exception& e) {
        m_store.updateState(assetId, api::DownloadState::Failed,
            QString::fromUtf8(e.what()));
        throw;
    }

    m_server->registerSession(raw);
    m_sessions.emplace(assetId, std::move(session));

    item.state = api::DownloadState::Streaming;
    item.expectedSizeBytes = raw->fileSize();
    m_store.upsert(item);
    Q_EMIT itemChanged(assetId);

    co_return m_server->urlFor(raw);
}

void DownloadManager::installHttpProgressBindings(HttpAssetSession* raw,
    const QString& assetId)
{
    connect(raw, &AssetSession::cachedBytesChanged,
        this, [this, assetId](qint64 bytes) {
            m_store.updateProgress(assetId, bytes, false);
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
            << "http asset completed assetId=" << assetId;
    });
    connect(raw, &AssetSession::failed, this,
        [this, assetId](const QString& reason) {
            m_store.updateState(assetId, api::DownloadState::Failed, reason);
            m_liveStats.erase(assetId);
            Q_EMIT itemChanged(assetId);
            qCWarning(KINEMA_DOWNLOAD).nospace()
                << "http asset failed assetId=" << assetId
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

void DownloadManager::installTorrentProgressBindings(
    TorrentAssetSession* raw, const QString& assetId)
{
    connect(raw, &AssetSession::cachedBytesChanged, this,
        [this, assetId](qint64 bytes) {
            // Promote Queued/Preparing rows to Downloading the
            // moment we see real bytes flowing.
            const auto found = m_store.find(assetId);
            const bool wasIdle = found
                && (found->state == api::DownloadState::Queued
                    || found->state == api::DownloadState::Preparing);
            m_store.updateProgress(assetId, bytes, false);
            if (wasIdle) {
                m_store.updateState(assetId,
                    api::DownloadState::Downloading);
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
            << "torrent asset completed assetId=" << assetId;
    });
    connect(raw, &AssetSession::failed, this,
        [this, assetId](const QString& reason) {
            m_store.updateState(assetId, api::DownloadState::Failed, reason);
            m_liveStats.erase(assetId);
            Q_EMIT itemChanged(assetId);
            qCWarning(KINEMA_DOWNLOAD).nospace()
                << "torrent asset failed assetId=" << assetId
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

void DownloadManager::enqueueDownload(const api::Stream& stream,
    const api::PlaybackContext& ctx,
    api::CacheDisposition disposition)
{
    const auto ref = api::assetRefFor(stream, ctx);
    if (!ref.isValid()) {
        Q_EMIT statusMessage(i18nc("@info:status",
            "Cannot enqueue: stream has no playable info hash."), 5000);
        qCWarning(KINEMA_DOWNLOAD)
            << "enqueueDownload rejected: invalid AssetRef (no info hash)";
        return;
    }
    const auto assetId = api::assetIdFor(ref);
    const auto backend = chooseBackend(stream);
    auto item = buildItem(ref, stream, ctx, backend, disposition);
    item.state = api::DownloadState::Queued;
    m_store.upsert(item);
    if (disposition == api::CacheDisposition::Pinned) {
        m_cache.setPinned(assetId, true);
    }
    Q_EMIT itemChanged(assetId);
    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "enqueueDownload assetId=" << assetId
        << " backend="
        << (backend == api::DownloadBackendKind::RealDebridHttp
                ? "RealDebridHttp"
                : "Torrent")
        << " disposition="
        << (disposition == api::CacheDisposition::Pinned
                ? "Pinned"
                : "Ephemeral");

    // Reject if a session already exists; the user shouldn't
    // double-enqueue.
    if (m_sessions.find(assetId) != m_sessions.end()) {
        qCInfo(KINEMA_DOWNLOAD)
            << "enqueueDownload: session already active for" << assetId;
        return;
    }

    if (backend == api::DownloadBackendKind::RealDebridHttp) {
        // Lazily realise the session and start prefetch. Wire the
        // store progress bindings so a background download is
        // actually visible to the UI — prior to this fix the
        // prefetch path emitted bytes into the void.
        auto session = std::make_unique<HttpAssetSession>(
            m_http, *m_resolver, m_settings, ref, assetId,
            m_cache.assetDir(assetId).absolutePath(), this);
        auto* raw = session.get();
        installHttpProgressBindings(raw, assetId);
        m_server->registerSession(raw);
        m_sessions.emplace(assetId, std::move(session));
        m_store.updateState(assetId, api::DownloadState::Preparing);
        Q_EMIT itemChanged(assetId);
        auto task = raw->prefetchAll();
        Q_UNUSED(task);
        return;
    }

    // Torrent backend: actually start the engine. Background mode
    // skips the streaming-buffer wait so the row transitions to
    // Downloading as soon as metadata + file selection complete,
    // and runs to completion regardless of whether the user plays.
    m_store.updateState(assetId, api::DownloadState::Preparing);
    Q_EMIT itemChanged(assetId);
    auto task = startTorrentBackground(ref, stream, ctx);
    Q_UNUSED(task);
}

QCoro::Task<void> DownloadManager::startTorrentBackground(
    api::AssetRef ref, api::Stream stream, api::PlaybackContext ctx)
{
    const auto assetId = api::assetIdFor(ref);
    try {
        m_cache.markActive(assetId);
        const auto prepared = co_await m_torrentEngine.prepareSession(
            stream, ctx, torrent::PrepareMode::Background);

        // The session may have been cancelled while we awaited.
        // Drop the prepared engine state if so.
        if (m_sessions.find(assetId) != m_sessions.end()) {
            // Already realised by a concurrent prepareForPlayback;
            // bind progress on the existing session and bail.
            qCInfo(KINEMA_DOWNLOAD)
                << "startTorrentBackground: session already realised"
                << assetId;
            co_return;
        }

        auto session = std::make_unique<TorrentAssetSession>(
            m_torrentEngine, assetId, prepared.token, prepared.fileName,
            prepared.fileSize, prepared.infoHash, this);
        auto* raw = session.get();
        installTorrentProgressBindings(raw, assetId);
        m_server->registerSession(raw);
        m_sessions.emplace(assetId, std::move(session));

        const auto persisted = m_store.find(assetId);
        const bool pinned = persisted
            && persisted->disposition == api::CacheDisposition::Pinned;
        if (pinned) {
            m_torrentEngine.setKeepAlive(prepared.infoHash, true);
        }

        if (auto row = m_store.find(assetId)) {
            row->expectedSizeBytes = prepared.fileSize;
            row->state = api::DownloadState::Downloading;
            m_store.upsert(*row);
        }
        Q_EMIT itemChanged(assetId);
        qCInfo(KINEMA_DOWNLOAD).nospace()
            << "startTorrentBackground started assetId=" << assetId
            << " fileSize=" << prepared.fileSize
            << " pinned=" << pinned;
    } catch (const std::exception& e) {
        const auto reason = QString::fromUtf8(e.what());
        m_store.updateState(assetId, api::DownloadState::Failed, reason);
        Q_EMIT itemChanged(assetId);
        qCWarning(KINEMA_DOWNLOAD).nospace()
            << "startTorrentBackground failed assetId=" << assetId
            << " reason=\"" << reason << "\"";
    }
}

void DownloadManager::pin(const QString& assetId, bool on)
{
    m_cache.setPinned(assetId, on);
    m_store.setDisposition(assetId,
        on ? api::CacheDisposition::Pinned
           : api::CacheDisposition::Ephemeral);
    // Mirror the pin onto the live torrent session so the engine's
    // idle-stop respects "Save offline" intent.
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
    qCInfo(KINEMA_DOWNLOAD) << "retry assetId=" << assetId
                            << "backend="
                            << static_cast<int>(found->backendKind);
    m_store.updateState(assetId, api::DownloadState::Queued, QString());
    Q_EMIT itemChanged(assetId);

    // Drop any half-broken session before restarting.
    if (auto it = m_sessions.find(assetId); it != m_sessions.end()) {
        m_server->unregisterSession(it->second.get());
        m_sessions.erase(it);
    }
    m_liveStats.erase(assetId);

    // Reconstruct enough of the original Stream + PlaybackContext
    // from the persisted row to drive the start helpers. We deliberately
    // synthesise a minimal Stream — the engine only needs releaseName
    // + infoHash to add the torrent.
    api::Stream stream;
    stream.infoHash = found->infoHash;
    stream.releaseName = found->releaseName;
    stream.qualityLabel = found->qualityLabel;
    stream.resolution = found->resolution;
    stream.provider = found->provider;
    if (found->expectedSizeBytes) {
        stream.sizeBytes = *found->expectedSizeBytes;
    }

    api::PlaybackContext ctx;
    ctx.key = found->key;
    ctx.title = found->title;
    ctx.seriesTitle = found->seriesTitle;
    ctx.episodeTitle = found->episodeTitle;
    ctx.poster = found->poster;

    api::AssetRef ref;
    ref.key = found->key;
    ref.infoHash = found->infoHash;
    ref.releaseName = found->releaseName;
    ref.fileIndex = found->fileIndex;
    ref.fileNameHint = found->fileNameHint;
    ref.qualityLabel = found->qualityLabel;
    ref.resolution = found->resolution;
    ref.provider = found->provider;
    ref.sizeBytes = found->expectedSizeBytes;

    if (found->backendKind == api::DownloadBackendKind::Torrent) {
        m_store.updateState(assetId, api::DownloadState::Preparing);
        Q_EMIT itemChanged(assetId);
        auto task = startTorrentBackground(ref, stream, ctx);
        Q_UNUSED(task);
    } else {
        // Re-realise the HTTP session and resume prefetch.
        auto session = std::make_unique<HttpAssetSession>(
            m_http, *m_resolver, m_settings, ref, assetId,
            m_cache.assetDir(assetId).absolutePath(), this);
        auto* raw = session.get();
        installHttpProgressBindings(raw, assetId);
        m_server->registerSession(raw);
        m_sessions.emplace(assetId, std::move(session));
        m_store.updateState(assetId, api::DownloadState::Preparing);
        Q_EMIT itemChanged(assetId);
        auto task = raw->prefetchAll();
        Q_UNUSED(task);
    }
}

void DownloadManager::runEviction()
{
    m_cache.enforceBudget();
}

} // namespace kinema::download
