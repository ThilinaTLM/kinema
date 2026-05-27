// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/transfer/SessionRegistry.h"

#include "playback/transfer/TransferSession.h"

namespace kinema::playback::transfer {

SessionRegistry::SessionRegistry(QObject* parent)
    : QObject(parent)
{
}

SessionRegistry::~SessionRegistry() = default;

TransferSession* SessionRegistry::find(const QString& assetId) const
{
    const auto it = m_sessions.find(assetId);
    return it == m_sessions.end() ? nullptr : it->second.get();
}

bool SessionRegistry::contains(const QString& assetId) const
{
    return m_sessions.find(assetId) != m_sessions.end();
}

int SessionRegistry::size() const noexcept
{
    return static_cast<int>(m_sessions.size());
}

TransferSession* SessionRegistry::registerSession(
    std::unique_ptr<TransferSession> session)
{
    Q_ASSERT(session);
    if (!session) {
        return nullptr;
    }
    const auto assetId = session->assetId();
    Q_ASSERT_X(!contains(assetId), "SessionRegistry::registerSession",
        "duplicate assetId; check find() before registering");
    auto* raw = session.get();
    m_sessions.emplace(assetId, std::move(session));
    Q_EMIT sessionRegistered(assetId);
    return raw;
}

std::unique_ptr<TransferSession> SessionRegistry::take(
    const QString& assetId)
{
    const auto it = m_sessions.find(assetId);
    if (it == m_sessions.end()) {
        return {};
    }
    auto session = std::move(it->second);
    m_sessions.erase(it);
    const bool hadAttached = m_attachedPlayers.remove(assetId);
    Q_EMIT sessionRemoved(assetId);
    if (hadAttached) {
        Q_EMIT attachedPlayerChanged(assetId, false);
    }
    return session;
}

void SessionRegistry::erase(const QString& assetId)
{
    auto removed = take(assetId);
    Q_UNUSED(removed);
}

void SessionRegistry::forEach(
    const std::function<void(TransferSession&)>& fn) const
{
    if (!fn) {
        return;
    }
    for (const auto& [assetId, session] : m_sessions) {
        Q_UNUSED(assetId);
        if (session) {
            fn(*session);
        }
    }
}

QStringList SessionRegistry::assetIds() const
{
    QStringList ids;
    ids.reserve(static_cast<int>(m_sessions.size()));
    for (const auto& [id, session] : m_sessions) {
        Q_UNUSED(session);
        ids.append(id);
    }
    return ids;
}

bool SessionRegistry::isOpening(const QString& assetId) const
{
    return m_opening.contains(assetId);
}

void SessionRegistry::markOpening(const QString& assetId)
{
    if (assetId.isEmpty()) {
        return;
    }
    m_opening.insert(assetId);
}

void SessionRegistry::clearOpening(const QString& assetId)
{
    m_opening.remove(assetId);
}

QStringList SessionRegistry::superseded(const QString& infoHash,
    const QString& keepAssetId) const
{
    QStringList out;
    if (infoHash.isEmpty()) {
        return out;
    }
    for (const auto& [assetId, session] : m_sessions) {
        if (!session) {
            continue;
        }
        if (assetId == keepAssetId) {
            continue;
        }
        if (session->infoHash().compare(infoHash, Qt::CaseInsensitive) != 0) {
            continue;
        }
        out.append(assetId);
    }
    return out;
}

bool SessionRegistry::attachPlayer(const QString& assetId)
{
    if (assetId.isEmpty()) {
        return false;
    }
    if (m_attachedPlayers.contains(assetId)) {
        return false;
    }
    m_attachedPlayers.insert(assetId);
    Q_EMIT attachedPlayerChanged(assetId, true);
    return true;
}

bool SessionRegistry::detachPlayer(const QString& assetId)
{
    if (assetId.isEmpty()) {
        return false;
    }
    if (!m_attachedPlayers.remove(assetId)) {
        return false;
    }
    Q_EMIT attachedPlayerChanged(assetId, false);
    return true;
}

bool SessionRegistry::hasAttachedPlayer(const QString& assetId) const
{
    return m_attachedPlayers.contains(assetId);
}

QVector<domain::MediaFileEntry> SessionRegistry::filesForStreamRef(
    const domain::HistoryStreamRef& streamRef) const
{
    if (streamRef.infoHash.isEmpty()) {
        return {};
    }
    for (const auto& [assetId, session] : m_sessions) {
        Q_UNUSED(assetId);
        if (!session) {
            continue;
        }
        if (session->infoHash().compare(streamRef.infoHash,
                Qt::CaseInsensitive)
            != 0) {
            continue;
        }
        auto files = session->files();
        if (!files.isEmpty()) {
            return files;
        }
    }
    return {};
}

QVector<domain::MediaFileEntry> SessionRegistry::filesForAssetId(
    const QString& assetId) const
{
    if (auto* s = find(assetId)) {
        return s->files();
    }
    return {};
}

} // namespace kinema::playback::transfer
