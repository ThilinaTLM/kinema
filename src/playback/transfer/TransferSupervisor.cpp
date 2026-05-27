// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/transfer/TransferSupervisor.h"

#include "domain/Download.h"
#include "kinema_log_download.h"
#include "playback/events/PlaybackEventStream.h"
#include "playback/ports/DownloadRepository.h"
#include "playback/transfer/SessionRegistry.h"
#include "playback/transfer/TransferSession.h"

#include <utility>

namespace kinema::playback::transfer {

TransferSupervisor::TransferSupervisor(SessionRegistry& registry,
    ports::DownloadRepository& repo,
    events::PlaybackEventStream& eventsBus,
    QObject* parent)
    : QObject(parent)
    , m_registry(registry)
    , m_repo(repo)
    , m_events(eventsBus)
    , m_resolver(
        [](const QString&) noexcept { return PlaybackSessionId {}; })
{
    connect(&m_registry, &SessionRegistry::sessionRegistered, this,
        &TransferSupervisor::onSessionRegistered);
    // Drop in-memory live stats when a session is taken / erased
    // without having reached terminal state (e.g. cancel + restart
    // of the same assetId).
    connect(&m_registry, &SessionRegistry::sessionRemoved, this,
        &TransferSupervisor::onSessionRemoved);
}

void TransferSupervisor::onSessionRemoved(const QString& assetId)
{
    m_liveStats.erase(assetId);
}

TransferSupervisor::~TransferSupervisor() = default;

void TransferSupervisor::setSessionIdResolver(SessionIdResolver fn)
{
    m_resolver = fn ? std::move(fn)
                    : [](const QString&) noexcept {
                          return PlaybackSessionId {};
                      };
}

PlaybackSessionId TransferSupervisor::resolveSessionId(
    const QString& assetId) const
{
    return m_resolver ? m_resolver(assetId) : PlaybackSessionId {};
}

void TransferSupervisor::onSessionRegistered(const QString& assetId)
{
    auto* session = m_registry.find(assetId);
    if (!session) {
        return;
    }
    // A fresh session starts with no telemetry. Wipe any stale
    // entry left over by a previous session for the same assetId.
    m_liveStats.erase(assetId);
    bind(session);
}

std::optional<LiveAssetStats> TransferSupervisor::liveStatsFor(
    const QString& assetId) const
{
    const auto it = m_liveStats.find(assetId);
    if (it == m_liveStats.end()) {
        return std::nullopt;
    }
    return it->second;
}

void TransferSupervisor::bind(TransferSession* session)
{
    Q_ASSERT(session);
    // Each lambda captures `this` and a raw `session` pointer
    // because Qt automatically severs the connection when the
    // session QObject is destroyed (the `context` overload below).
    connect(session, &TransferSession::cachedBytesChanged, this,
        [this, session](qint64 bytes) { onCachedBytes(session, bytes); });
    connect(session, &TransferSession::completed, this,
        [this, session] { onCompleted(session); });
    connect(session, &TransferSession::failed, this,
        [this, session](const QString& reason) {
            onFailed(session, reason);
        });
    connect(session, &TransferSession::liveStatsChanged, this,
        [this, session](qint64 rate, int peers, int seeds, int eta) {
            onLiveStats(session, rate, peers, seeds, eta);
        });
}

void TransferSupervisor::onCachedBytes(TransferSession* session,
    qint64 bytes)
{
    const auto assetId = session->assetId();
    const auto current = m_repo.find(assetId);
    const auto expected = session->fileSize();

    const bool wasIdle = current
        && (current->state == domain::DownloadState::Queued
            || current->state == domain::DownloadState::Resolving
            || current->state == domain::DownloadState::Idle);

    m_repo.updateCachedBytes(assetId, bytes,
        expected > 0 ? std::optional<qint64>(expected) : std::nullopt,
        false);
    if (wasIdle) {
        m_repo.updateState(assetId, domain::DownloadState::Active);
    }

    m_events.publish(events::TransferProgressed {
        .sessionId = resolveSessionId(assetId),
        .assetId = assetId,
        .cachedBytes = bytes,
        .expectedBytes = expected,
    });
    Q_EMIT itemChanged(assetId);
}

void TransferSupervisor::onCompleted(TransferSession* session)
{
    const auto assetId = session->assetId();
    const auto expected = session->fileSize();

    m_repo.updateCachedBytes(assetId,
        expected > 0 ? expected : 0,
        expected > 0 ? std::optional<qint64>(expected) : std::nullopt,
        true);
    m_repo.updateState(assetId, domain::DownloadState::Completed);
    // The legacy `DownloadManager::installProgressBindings` wiped
    // live stats on terminal events. Mirror that so the Downloads
    // page stops showing a stale rate / peers chip on completed
    // rows.
    m_liveStats.erase(assetId);

    m_events.publish(events::TransferCompleted {
        .sessionId = resolveSessionId(assetId),
        .assetId = assetId,
    });
    Q_EMIT itemChanged(assetId);

    qCInfo(KINEMA_DOWNLOAD).nospace()
        << "TransferSupervisor: asset completed assetId=" << assetId;
}

void TransferSupervisor::onFailed(TransferSession* session,
    const QString& reason)
{
    const auto assetId = session->assetId();
    m_repo.setLastError(assetId, reason);
    m_liveStats.erase(assetId);

    m_events.publish(events::TransferFailed {
        .sessionId = resolveSessionId(assetId),
        .assetId = assetId,
        .reason = reason,
    });
    Q_EMIT itemChanged(assetId);

    qCWarning(KINEMA_DOWNLOAD).nospace()
        << "TransferSupervisor: asset failed assetId=" << assetId
        << " reason=\"" << reason << "\"";
}

void TransferSupervisor::onLiveStats(TransferSession* session,
    qint64 rate, int peers, int seeds, int eta)
{
    const auto assetId = session->assetId();
    auto& slot = m_liveStats[assetId];
    slot.ratePayloadBps = rate;
    slot.peers = peers;
    slot.seeds = seeds;
    slot.etaSeconds = eta;

    m_events.publish(events::TransferStatsChanged {
        .sessionId = resolveSessionId(assetId),
        .assetId = assetId,
        .bytesPerSec = rate,
        .peers = peers,
        .seeds = seeds,
    });
    // Live stats don't touch the repository; only the in-memory
    // `m_liveStats` map and the typed event are updated. The
    // legacy `DownloadManager::installProgressBindings` did the
    // same.
}

} // namespace kinema::playback::transfer
