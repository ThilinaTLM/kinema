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
#include "kinema_debug.h"
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
        qCWarning(KINEMA) << "DownloadManager: could not bind localhost server";
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
    if (rdConfigured && (s.rdCached || !s.directUrl.isEmpty())) {
        return api::DownloadBackendKind::RealDebridHttp;
    }
    return api::DownloadBackendKind::Torrent;
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

    auto item = buildItem(ref, stream, ctx,
        api::DownloadBackendKind::Torrent,
        api::CacheDisposition::Ephemeral);
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

    const auto prepared = co_await m_torrentEngine.prepareSession(stream, ctx);
    auto session = std::make_unique<TorrentAssetSession>(m_torrentEngine,
        assetId, prepared.token, prepared.fileName, prepared.fileSize,
        prepared.infoHash, this);
    auto* raw = session.get();
    m_server->registerSession(raw);
    m_sessions.emplace(assetId, std::move(session));

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

    auto item = buildItem(ref, stream, ctx,
        api::DownloadBackendKind::RealDebridHttp,
        api::CacheDisposition::Ephemeral);
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
        Q_EMIT itemChanged(assetId);
    });
    connect(raw, &AssetSession::failed,
        this, [this, assetId](const QString& reason) {
            m_store.updateState(assetId, api::DownloadState::Failed, reason);
            Q_EMIT itemChanged(assetId);
        });

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

void DownloadManager::enqueueDownload(const api::Stream& stream,
    const api::PlaybackContext& ctx,
    api::CacheDisposition disposition)
{
    const auto ref = api::assetRefFor(stream, ctx);
    if (!ref.isValid()) {
        Q_EMIT statusMessage(i18nc("@info:status",
            "Cannot enqueue: stream has no playable info hash."), 5000);
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
    // Background prefetch dispatch for HTTP backend; torrent
    // background prefetch is best-effort \u2014 the torrent engine
    // already prioritises its file selection.
    if (backend == api::DownloadBackendKind::RealDebridHttp) {
        // Lazily realise the session and start prefetch.
        auto session = std::make_unique<HttpAssetSession>(
            m_http, *m_resolver, m_settings, ref, assetId,
            m_cache.assetDir(assetId).absolutePath(), this);
        auto* raw = session.get();
        m_sessions.emplace(assetId, std::move(session));
        auto task = raw->prefetchAll();
        Q_UNUSED(task);
    }
}

void DownloadManager::pin(const QString& assetId, bool on)
{
    m_cache.setPinned(assetId, on);
    m_store.setDisposition(assetId,
        on ? api::CacheDisposition::Pinned
           : api::CacheDisposition::Ephemeral);
    Q_EMIT itemChanged(assetId);
}

void DownloadManager::cancel(const QString& assetId)
{
    if (auto it = m_sessions.find(assetId); it != m_sessions.end()) {
        m_server->unregisterSession(it->second.get());
        m_sessions.erase(it);
    }
    m_cache.markInactive(assetId);
    m_store.updateState(assetId, api::DownloadState::Cancelled);
    Q_EMIT itemChanged(assetId);
}

void DownloadManager::remove(const QString& assetId, bool deleteFiles)
{
    if (auto it = m_sessions.find(assetId); it != m_sessions.end()) {
        m_server->unregisterSession(it->second.get());
        m_sessions.erase(it);
    }
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
        return;
    }
    m_store.updateState(assetId, api::DownloadState::Queued, QString());
    Q_EMIT itemChanged(assetId);
}

void DownloadManager::runEviction()
{
    m_cache.enforceBudget();
}

} // namespace kinema::download
