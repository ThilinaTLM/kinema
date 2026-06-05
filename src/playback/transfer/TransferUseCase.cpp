// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/transfer/TransferUseCase.h"

#include "core/persistence/DownloadStore.h" // synthesiseStartArgs
#include "core/persistence/MediaCache.h"
#include "kinema_log_download.h"
#include "playback/policy/BackendSelectionPolicy.h"
#include "playback/ports/DownloadRepository.h"
#include "playback/ports/MediaSourcePort.h"
#include "playback/sources/AssetSession.h"
#include "playback/streaming/LocalHttpStreamGateway.h"
#include "playback/transfer/BackendRegistry.h"
#include "playback/transfer/SessionRegistry.h"
#include "playback/transfer/TransferSession.h"
#include "playback/transfer/TransferSupervisor.h"
#include "playback/torrent/LibtorrentClient.h"

#include <KLocalizedString>

#include <QCoro/QCoroSignal>
#include <QTimer>

#include <stdexcept>
#include <utility>

namespace kinema::playback::transfer {

namespace {

const char* modeName(domain::DownloadMode m)
{
    return m == domain::DownloadMode::Full ? "Full" : "OnDemand";
}

const char* dispositionName(domain::CacheDisposition d)
{
    return d == domain::CacheDisposition::Pinned ? "Pinned"
                                                 : "Ephemeral";
}

const char* backendName(domain::DownloadBackendKind k)
{
    switch (k) {
    case domain::DownloadBackendKind::RealDebridHttp:
        return "RealDebridHttp";
    case domain::DownloadBackendKind::AllDebridHttp:
        return "AllDebridHttp";
    case domain::DownloadBackendKind::Torrent:
        break;
    }
    return "Torrent";
}

QCoro::Task<void> sleepMs(int ms)
{
    QTimer timer;
    timer.setSingleShot(true);
    timer.start(ms);
    co_await qCoro(&timer, &QTimer::timeout);
    co_return;
}

/// Translate a `BackendSelectionError` to the localised status
/// message the legacy `DownloadManager` threw. Kept compatible so
/// existing UI strings (and the resulting `lastError` row text)
/// don't shift under the cutover.
QString localisedSelectionError(const policy::BackendSelectionError& e)
{
    using K = policy::BackendSelectionError::Kind;
    switch (e.kind) {
    case K::OverrideUnsupported:
        return i18nc("@info:status",
            "Requested backend is not available for this stream.");
    case K::ActiveProviderUnsupported:
        return i18nc("@info:status",
            "The active debrid provider can't serve this stream.");
    case K::NoBackend:
    default:
        return i18nc("@info:status",
            "No download backend can serve this stream.");
    }
}

} // namespace

TransferUseCase::TransferUseCase(BackendRegistry& backends,
    SessionRegistry& sessions,
    TransferSupervisor& supervisor,
    streaming::LocalHttpStreamGateway& gateway,
    ports::DownloadRepository& repo,
    core::MediaCache& cache,
    playback::torrent::LibtorrentClient& torrentEngine,
    QObject* parent)
    : QObject(parent)
    , m_backends(backends)
    , m_sessions(sessions)
    , m_supervisor(supervisor)
    , m_gateway(gateway)
    , m_repo(repo)
    , m_cache(cache)
    , m_torrentEngine(torrentEngine)
{
    // Re-emit the supervisor's per-row mutation hint so QML
    // view-models can keep listening on a single source. The
    // controller forwards `itemChanged` straight through to
    // delegates that need a `dataChanged`-only refresh.
    connect(&m_supervisor, &TransferSupervisor::itemChanged,
        this, &TransferUseCase::itemChanged);
}

TransferUseCase::~TransferUseCase() = default;

domain::DownloadItem TransferUseCase::buildItem(
    const domain::AssetRef& ref, const domain::Stream& s,
    const domain::PlaybackContext& ctx,
    domain::DownloadBackendKind backend, domain::DownloadMode mode,
    domain::CacheDisposition disposition) const
{
    domain::DownloadItem it;
    it.assetId = domain::assetIdFor(ref);
    it.backendKind = backend;
    it.state = domain::DownloadState::Resolving;
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

void TransferUseCase::supersedeSameHashSessions(const QString& infoHash,
    const QString& keepAssetId)
{
    if (infoHash.isEmpty()) {
        return;
    }
    const auto victims = m_sessions.superseded(infoHash, keepAssetId);
    for (const auto& otherAssetId : victims) {
        auto* session = m_sessions.find(otherAssetId);
        if (!session) {
            continue;
        }
        if (session->backendKind()
            != domain::DownloadBackendKind::Torrent) {
            qCDebug(KINEMA_DOWNLOAD).nospace()
                << "supersedeSameHashSessions: keeping non-torrent "
                << "assetId=" << otherAssetId;
            continue;
        }

        m_gateway.revoke(*session->byteRangeSource());
        m_sessions.erase(otherAssetId); // also drops attached-player
        // Mirror the legacy OnDemand→Idle bookkeeping so the
        // Downloads page reflects the swap rather than leaving a
        // ghost "Active" row.
        if (const auto row = m_repo.find(otherAssetId);
            row && row->mode == domain::DownloadMode::OnDemand
            && row->state == domain::DownloadState::Active) {
            m_repo.updateState(otherAssetId,
                domain::DownloadState::Idle);
            Q_EMIT itemChanged(otherAssetId);
        }
        m_cache.markInactive(otherAssetId);
    }
}

QCoro::Task<QUrl> TransferUseCase::ensurePlayable(
    PlaybackSessionId sessionId,
    domain::Stream stream,
    domain::PlaybackContext ctx,
    std::optional<domain::DownloadBackendKind> backendOverride)
{
    const auto ref = domain::assetRefFor(stream, ctx);
    if (!ref.isValid()) {
        // Direct-URL-only candidate without an info hash. We have
        // no way to expose it on the gateway, so fall back to the
        // caller's "play raw URL" branch via the same exception
        // contract `DownloadManager::prepareForPlayback` used.
        throw std::runtime_error(i18nc("@info:status",
            "This stream has no playable info hash.").toStdString());
    }
    const auto assetId = domain::assetIdFor(ref);

    // Preserve a previously persisted row's mode + disposition.
    // A second click on Play does not downgrade an existing Full
    // session.
    const auto existing = m_repo.find(assetId);
    const auto mode = existing
        ? existing->mode
        : domain::DownloadMode::OnDemand;
    const auto disposition = existing
        ? existing->disposition
        : domain::CacheDisposition::Ephemeral;

    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "ensurePlayable assetId=" << assetId
        << " mode=" << modeName(mode)
        << " disposition=" << dispositionName(disposition);

    const auto url = co_await openSession(ref, std::move(stream),
        std::move(ctx), mode, disposition, backendOverride, sessionId);
    attachPlayer(assetId);
    co_return url;
}

void TransferUseCase::saveOffline(domain::Stream stream,
    domain::PlaybackContext ctx,
    std::optional<domain::DownloadBackendKind> backendOverride)
{
    const auto ref = domain::assetRefFor(stream, ctx);
    if (!ref.isValid()) {
        Q_EMIT statusMessage(i18nc("@info:status",
            "Cannot download: stream has no playable info hash."), 5000);
        qCWarning(KINEMA_DOWNLOAD)
            << "saveOffline rejected: invalid AssetRef (no info hash)";
        return;
    }
    const auto assetId = domain::assetIdFor(ref);

    // Already-active session: upgrade in place rather than spawning
    // a duplicate. Mirrors `DownloadManager::enqueueDownload`.
    if (m_sessions.contains(assetId)) {
        upgradeToFull(assetId);
        return;
    }

    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "saveOffline assetId=" << assetId
        << " mode=Full disposition=Pinned";

    auto task = startBackground(ref, std::move(stream), std::move(ctx),
        domain::DownloadMode::Full, domain::CacheDisposition::Pinned,
        backendOverride);
    Q_UNUSED(task);
}

void TransferUseCase::upgradeToFull(const QString& assetId)
{
    auto* session = m_sessions.find(assetId);
    if (!session) {
        qCInfo(KINEMA_DOWNLOAD)
            << "upgradeToFull: no active session for" << assetId;
        return;
    }
    if (session->mode() == domain::DownloadMode::Full) {
        // Still ensure pin marker is set in case the user pinned
        // by hand earlier and we lost track.
        m_cache.setPinned(assetId, true);
        m_repo.setDisposition(assetId,
            domain::CacheDisposition::Pinned);
        return;
    }

    auto* source = m_backends.find(session->backendKind());
    if (!source) {
        qCWarning(KINEMA_DOWNLOAD)
            << "upgradeToFull: no backend registered for" << assetId;
        return;
    }
    auto* src = session->byteRangeSource();
    if (!src) {
        return;
    }
    source->changeMode(*src, domain::DownloadMode::Full);
    session->setMode(domain::DownloadMode::Full);
    session->setDisposition(domain::CacheDisposition::Pinned);

    m_repo.updateMode(assetId, domain::DownloadMode::Full);
    m_cache.setPinned(assetId, true);
    m_repo.setDisposition(assetId, domain::CacheDisposition::Pinned);
    Q_EMIT itemChanged(assetId);

    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "upgradeToFull assetId=" << assetId
        << " (OnDemand -> Full + Pinned)";
}

void TransferUseCase::attachPlayer(const QString& assetId)
{
    if (assetId.isEmpty()) {
        return;
    }
    if (!m_sessions.attachPlayer(assetId)) {
        return; // already attached
    }
    // OnDemand rows previously parked at Idle return to Active when
    // a consumer reattaches. Full rows ignore the flag because they
    // never sit at Idle.
    if (auto row = m_repo.find(assetId);
        row && row->state == domain::DownloadState::Idle) {
        m_repo.updateState(assetId, domain::DownloadState::Active);
    }
    Q_EMIT itemChanged(assetId);
}

void TransferUseCase::detachPlayer(const QString& assetId)
{
    if (assetId.isEmpty()) {
        return;
    }
    if (!m_sessions.detachPlayer(assetId)) {
        return;
    }
    if (auto row = m_repo.find(assetId)) {
        if (row->mode == domain::DownloadMode::OnDemand
            && row->state == domain::DownloadState::Active) {
            // Park at Idle so the UI can show that we're alive but
            // not currently fetching for a consumer. The torrent
            // engine's own idle-stop timer will eventually quiesce
            // the libtorrent handle (governed by
            // TorrentStreamingSettings::idleStopMinutes).
            m_repo.updateState(assetId, domain::DownloadState::Idle);
        }
    }
    Q_EMIT itemChanged(assetId);
}

void TransferUseCase::pause(const QString& assetId)
{
    if (auto* session = m_sessions.find(assetId)) {
        session->pause();
    }
    m_repo.updateState(assetId, domain::DownloadState::Paused);
    Q_EMIT itemChanged(assetId);
    qCInfo(KINEMA_DOWNLOAD) << "pause assetId=" << assetId;
}

void TransferUseCase::resumeTransfer(const QString& assetId)
{
    if (auto* session = m_sessions.find(assetId)) {
        session->resume();
        m_repo.updateState(assetId, domain::DownloadState::Active);
        Q_EMIT itemChanged(assetId);
        qCInfo(KINEMA_DOWNLOAD) << "resume assetId=" << assetId;
        return;
    }
    // No live session — kick a retry.
    retry(assetId);
}

void TransferUseCase::retry(const QString& assetId)
{
    const auto found = m_repo.find(assetId);
    if (!found) {
        qCInfo(KINEMA_DOWNLOAD)
            << "retry: no persisted row for assetId=" << assetId;
        return;
    }
    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "retry assetId=" << assetId
        << " backend=" << backendName(found->backendKind)
        << " mode=" << modeName(found->mode);

    m_repo.updateState(assetId, domain::DownloadState::Queued);
    Q_EMIT itemChanged(assetId);

    // Drop any half-broken session before restarting.
    if (auto* session = m_sessions.find(assetId)) {
        m_gateway.revoke(*session->byteRangeSource());
    }
    m_sessions.erase(assetId);

    auto args = core::DownloadStore::synthesiseStartArgs(*found);
    auto task = startBackground(args.ref, args.stream, args.ctx,
        found->mode, found->disposition, found->backendKind);
    Q_UNUSED(task);
}

void TransferUseCase::cancel(const QString& assetId)
{
    qCInfo(KINEMA_DOWNLOAD) << "cancel assetId=" << assetId;
    const auto row = m_repo.find(assetId);
    if (row && !row->infoHash.isEmpty()) {
        m_torrentEngine.stopInfoHash(row->infoHash);
    }
    if (auto* session = m_sessions.find(assetId)) {
        m_gateway.revoke(*session->byteRangeSource());
    }
    m_sessions.erase(assetId);
    m_cache.markInactive(assetId);
    m_repo.updateState(assetId, domain::DownloadState::Cancelled);
    Q_EMIT itemChanged(assetId);
}

void TransferUseCase::remove(const QString& assetId, bool deleteFiles)
{
    qCInfo(KINEMA_DOWNLOAD) << "remove assetId=" << assetId
                            << "deleteFiles=" << deleteFiles;
    const auto row = m_repo.find(assetId);
    if (row && !row->infoHash.isEmpty()) {
        m_torrentEngine.stopInfoHash(row->infoHash);
    }
    if (auto* session = m_sessions.find(assetId)) {
        m_gateway.revoke(*session->byteRangeSource());
    }
    m_sessions.erase(assetId);
    m_cache.markInactive(assetId);
    m_cache.setPinned(assetId, false);
    if (deleteFiles) {
        m_cache.removeAsset(assetId);
    }
    m_repo.remove(assetId);
}

void TransferUseCase::pin(const QString& assetId, bool on)
{
    m_cache.setPinned(assetId, on);
    m_repo.setDisposition(assetId,
        on ? domain::CacheDisposition::Pinned
           : domain::CacheDisposition::Ephemeral);
    // Mirror the pin onto the live torrent session so the engine's
    // idle-stop respects "Save offline" intent. Other backends are
    // no-ops here.
    if (const auto found = m_repo.find(assetId);
        found && !found->infoHash.isEmpty()) {
        m_torrentEngine.setKeepAlive(found->infoHash, on);
    }
    if (auto* session = m_sessions.find(assetId)) {
        session->setDisposition(on
                ? domain::CacheDisposition::Pinned
                : domain::CacheDisposition::Ephemeral);
    }
    qCInfo(KINEMA_DOWNLOAD)
        << "pin assetId=" << assetId << "-> on=" << on;
    Q_EMIT itemChanged(assetId);
}

void TransferUseCase::resumePersisted()
{
    using S = domain::DownloadState;
    using M = domain::DownloadMode;

    int resumed = 0;
    for (const auto& row : m_repo.all()) {
        if (row.mode != M::Full) {
            continue;
        }
        if (row.state == S::Completed || row.state == S::Failed
            || row.state == S::Cancelled) {
            continue;
        }
        if (m_sessions.contains(row.assetId)) {
            continue;
        }

        // Normalise stale mid-flight states from the previous run.
        m_repo.updateState(row.assetId, S::Queued);

        auto args = core::DownloadStore::synthesiseStartArgs(row);
        auto task = startBackground(args.ref, args.stream, args.ctx,
            M::Full, row.disposition, row.backendKind);
        Q_UNUSED(task);
        ++resumed;
    }
    qCInfo(KINEMA_DOWNLOAD)
        << "resumePersisted: kicked" << resumed << "Full sessions";
}

std::optional<domain::DownloadItem> TransferUseCase::findForKey(
    const domain::PlaybackKey& key) const
{
    return m_repo.findForKey(key);
}

std::optional<LiveAssetStats> TransferUseCase::liveStatsFor(
    const QString& assetId) const
{
    return m_supervisor.liveStatsFor(assetId);
}

QSet<QString> TransferUseCase::attachedPlayerAssetIds() const
{
    return m_sessions.attachedPlayerAssetIds();
}

QCoro::Task<ports::ByteRangeSource*>
TransferUseCase::ensureSessionForAssetId(const QString& assetId)
{
    if (assetId.isEmpty()) {
        co_return nullptr;
    }
    if (auto* session = m_sessions.find(assetId)) {
        session->touch();
        co_return session->byteRangeSource();
    }

    const auto row = m_repo.find(assetId);
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

    if (auto* session = m_sessions.find(assetId)) {
        co_return session->byteRangeSource();
    }
    co_return nullptr;
}

QVector<domain::MediaFileEntry> TransferUseCase::filesForStreamRef(
    const domain::HistoryStreamRef& streamRef) const
{
    return m_sessions.filesForStreamRef(streamRef);
}

QVector<domain::MediaFileEntry> TransferUseCase::filesForAssetId(
    const QString& assetId) const
{
    return m_sessions.filesForAssetId(assetId);
}

QCoro::Task<QUrl> TransferUseCase::openSession(domain::AssetRef ref,
    domain::Stream stream, domain::PlaybackContext ctx,
    domain::DownloadMode mode, domain::CacheDisposition disposition,
    std::optional<domain::DownloadBackendKind> backendOverride,
    PlaybackSessionId sessionId)
{
    const auto assetId = domain::assetIdFor(ref);

    // Reuse an active session if one exists for this exact asset.
    if (auto* session = m_sessions.find(assetId)) {
        if (!sessionId.isNull()) {
            session->setPlaybackSessionId(sessionId);
        }
        session->touch();
        co_return m_gateway.urlFor(session->assetId());
    }

    // Series-pack episode navigation can request a different file
    // inside the same info hash. The torrent backend only serves
    // one selected file per live hash session, so supersede any
    // older same-hash asset sessions before opening the new one.
    supersedeSameHashSessions(ref.infoHash, assetId);

    while (m_sessions.isOpening(assetId)) {
        if (auto* session = m_sessions.find(assetId)) {
            if (!sessionId.isNull()) {
                session->setPlaybackSessionId(sessionId);
            }
            session->touch();
            co_return m_gateway.urlFor(session->assetId());
        }
        co_await sleepMs(25);
    }
    if (auto* session = m_sessions.find(assetId)) {
        if (!sessionId.isNull()) {
            session->setPlaybackSessionId(sessionId);
        }
        session->touch();
        co_return m_gateway.urlFor(session->assetId());
    }

    m_sessions.markOpening(assetId);

    try {
        const auto result = m_backends.select(stream, backendOverride);
        if (const auto* err
            = std::get_if<policy::BackendSelectionError>(&result)) {
            throw std::runtime_error(
                localisedSelectionError(*err).toStdString());
        }
        auto* backend = std::get<ports::MediaSourcePort*>(result);
        Q_ASSERT(backend);

        // Persist the row immediately so the UI shows a Resolving
        // entry while the backend opens its session.
        auto item = buildItem(ref, stream, ctx, backend->kind(),
            mode, disposition);
        m_repo.upsert(item);
        if (disposition == domain::CacheDisposition::Pinned) {
            m_cache.setPinned(assetId, true);
        }

        Q_EMIT statusMessage(mode == domain::DownloadMode::Full
                ? i18nc("@info:status", "Starting download\u2026")
                : i18nc("@info:status", "Buffering stream\u2026"),
            0);

        auto opened = co_await backend->open(ref, stream, ctx, mode);

        auto session = std::make_unique<TransferSession>(ref, ctx,
            backend->kind(), mode, disposition,
            std::move(opened.session), sessionId);
        auto* raw = session.get();

        // Register first so the supervisor wires its progress
        // bindings before we expose the source on the gateway
        // (otherwise the first `cachedBytesChanged` emit could
        // race past the supervisor).
        m_sessions.registerSession(std::move(session));
        m_gateway.expose(*raw->byteRangeSource());

        // Seed the persisted progress from whatever the session
        // discovered on disk (HttpRangeAssetSession::loadChunkMap,
        // or a torrent resume populated by the engine). Without
        // this, re-opening a session over an already-cached
        // payload leaves cached_size_bytes at 0 forever —
        // fetchChunk only emits cachedBytesChanged on chunks it
        // actually downloads, so a fully-cached play stays at 0%
        // on the Downloads row.
        const qint64 initialCached = raw->cachedBytes();
        const qint64 expected = raw->fileSize();
        item.state = domain::DownloadState::Active;
        if (expected > 0) {
            item.expectedSizeBytes = expected;
        }
        if (initialCached >= 0) {
            item.cachedSizeBytes = initialCached;
            if (expected > 0 && initialCached >= expected) {
                item.complete = true;
                item.state = domain::DownloadState::Completed;
            }
        }
        m_repo.upsert(item);
        Q_EMIT itemChanged(assetId);

        const QUrl url = m_gateway.urlFor(raw->assetId());
        m_sessions.clearOpening(assetId);
        co_return url;
    } catch (...) {
        m_sessions.clearOpening(assetId);
        throw;
    }
}

QCoro::Task<void> TransferUseCase::startBackground(domain::AssetRef ref,
    domain::Stream stream, domain::PlaybackContext ctx,
    domain::DownloadMode mode, domain::CacheDisposition disposition,
    std::optional<domain::DownloadBackendKind> backendOverride,
    PlaybackSessionId sessionId)
{
    const auto assetId = domain::assetIdFor(ref);
    try {
        co_await openSession(std::move(ref), std::move(stream),
            std::move(ctx), mode, disposition, backendOverride, sessionId);
    } catch (const std::exception& e) {
        const auto reason = QString::fromUtf8(e.what());
        m_repo.updateState(assetId, domain::DownloadState::Failed);
        m_repo.setLastError(assetId, reason);
        Q_EMIT itemChanged(assetId);
        qCWarning(KINEMA_DOWNLOAD).nospace()
            << "startBackground failed assetId=" << assetId
            << " reason=\"" << reason << "\"";
    }
}

} // namespace kinema::playback::transfer
