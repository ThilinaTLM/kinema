// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/MediaFile.h"
#include "domain/PlaybackContext.h"
#include "playback/ports/SessionFileCatalog.h"

#include <QObject>
#include <QSet>
#include <QString>
#include <QStringList>
#include <QVector>

#include <functional>
#include <map>
#include <memory>

namespace kinema::playback::transfer {

class TransferSession;

/**
 * Owns active `TransferSession` instances keyed by `assetId`.
 *
 * Provides the in-memory bookkeeping the legacy
 * `download::DownloadManager` did inline:
 *
 *  - asset-id map of live sessions;
 *  - opening-guard set (prevents duplicate creation when multiple
 *    `prepareForPlayback` calls race a cold recovery);
 *  - attached-player set so OnDemand rows can transition between
 *    Active and Idle;
 *  - same-infoHash supersede lookup for series-pack episode
 *    navigation (the torrent backend can only serve one file per
 *    live hash session).
 *
 * Implements `ports::SessionFileCatalog` so series adjacency can
 * resolve through one abstraction regardless of which backend
 * opened the session.
 *
 * The registry is intentionally backend-agnostic: it knows nothing
 * about libtorrent, debrid resolution, or persistent storage.
 * Higher-level helpers (`TransferSupervisor`, `BackendRegistry`,
 * `TransferUseCase`) compose those concerns on top.
 */
class SessionRegistry : public QObject,
    public ports::SessionFileCatalog
{
    Q_OBJECT
public:
    explicit SessionRegistry(QObject* parent = nullptr);
    ~SessionRegistry() override;

    SessionRegistry(const SessionRegistry&) = delete;
    SessionRegistry& operator=(const SessionRegistry&) = delete;

    /// Borrow the live session for `assetId`, or nullptr.
    TransferSession* find(const QString& assetId) const;

    /// True when a session is registered for `assetId`.
    bool contains(const QString& assetId) const;

    /// Number of registered sessions.
    int size() const noexcept;

    /// Insert and take ownership. Asserts on duplicate `assetId`;
    /// callers must check `find()` first when they need merge
    /// semantics. Returns a borrowed pointer to the inserted
    /// session.
    TransferSession* registerSession(std::unique_ptr<TransferSession> session);

    /// Remove and return ownership of the session. Returns nullptr
    /// when no session is registered. Clears any "attached player"
    /// marker for the asset.
    std::unique_ptr<TransferSession> take(const QString& assetId);

    /// Convenience: remove and destroy.
    void erase(const QString& assetId);

    /// Iterate over every live session. Callback may not mutate the
    /// registry during iteration.
    void forEach(const std::function<void(TransferSession&)>& fn) const;

    /// All currently-registered asset ids in registration order
    /// (stable; backed by `std::map`).
    QStringList assetIds() const;

    // --- opening guard -----------------------------------------

    bool isOpening(const QString& assetId) const;
    void markOpening(const QString& assetId);
    void clearOpening(const QString& assetId);

    // --- same-infoHash supersede -------------------------------

    /**
     * Return asset ids of every live session whose `infoHash`
     * matches `infoHash` (case-insensitive) except for
     * `keepAssetId`. Callers use this to revoke superseded
     * sessions before opening a new file inside the same hash —
     * the registry itself does not erase them so the caller can
     * tear down gateway exposure, store rows, and live stats
     * before letting the `TransferSession` destruct.
     */
    QStringList superseded(const QString& infoHash,
        const QString& keepAssetId) const;

    // --- attached-player set -----------------------------------

    /// Mark a player as attached. Returns `true` if newly attached.
    bool attachPlayer(const QString& assetId);
    /// Returns `true` if a player was previously attached.
    bool detachPlayer(const QString& assetId);
    bool hasAttachedPlayer(const QString& assetId) const;
    QSet<QString> attachedPlayerAssetIds() const { return m_attachedPlayers; }

    // --- ports::SessionFileCatalog -----------------------------

    QVector<domain::MediaFileEntry> filesForStreamRef(
        const domain::HistoryStreamRef& streamRef) const override;
    QVector<domain::MediaFileEntry> filesForAssetId(
        const QString& assetId) const override;

Q_SIGNALS:
    /// A session was registered. The signal fires before any
    /// downstream wiring runs so subscribers can install bindings
    /// (e.g. `TransferSupervisor` listening for progress).
    void sessionRegistered(QString assetId);

    /// A session was removed (via `take` / `erase`). Fired after
    /// the entry is gone from the map.
    void sessionRemoved(QString assetId);

    /// A player attached or detached.
    void attachedPlayerChanged(QString assetId, bool attached);

private:
    std::map<QString, std::unique_ptr<TransferSession>> m_sessions;
    QSet<QString> m_opening;
    QSet<QString> m_attachedPlayers;
};

} // namespace kinema::playback::transfer
