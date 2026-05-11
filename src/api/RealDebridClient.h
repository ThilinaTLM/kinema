// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/RealDebrid.h"

#include <QList>
#include <QObject>
#include <QString>
#include <QUrl>

#include <QCoro/QCoroTask>

namespace kinema::core {
class HttpClient;
}

namespace kinema::api {
using namespace kinema::domain;

/**
 * Client for the Real-Debrid REST v1 API.
 *
 * Kinema uses RD for two things now:
 *
 * 1. Validating the user-supplied token (`/user`).
 * 2. Decoupled, on-demand torrent resolution for the unified downloader:
 *    `addMagnet` → `torrentInfo` → `selectFiles` → `torrentInfo`
 *    → `unrestrictLink`. The download subsystem orchestrates that
 *    workflow inside `RealDebridResolver`. RD's old
 *    `/torrents/instantAvailability/{hash}` probe was deprecated
 *    upstream (returns 403 / empty objects in practice) and is no
 *    longer part of our flow.
 *
 * Endpoints used:
 *   GET  /rest/1.0/user
 *   POST /rest/1.0/torrents/addMagnet      (form-encoded)
 *   GET  /rest/1.0/torrents/info/{id}
 *   POST /rest/1.0/torrents/selectFiles/{id} (form-encoded)
 *   POST /rest/1.0/unrestrict/link         (form-encoded)
 *   DELETE /rest/1.0/torrents/delete/{id}
 *
 * Every request carries `Authorization: Bearer <token>`.
 */
class RealDebridClient : public QObject
{
    Q_OBJECT
public:
    explicit RealDebridClient(core::HttpClient* http, QObject* parent = nullptr);

    /// Override the RD base URL (default: https://api.real-debrid.com/rest/1.0).
    void setBaseUrl(QUrl url);
    const QUrl& baseUrl() const noexcept { return m_baseUrl; }

    /// In-memory only. The token is supplied by the caller per-session.
    void setToken(QString token);
    const QString& token() const noexcept { return m_token; }

    /// Throws HttpError. Auth failures surface as HttpStatus 401.
    virtual QCoro::Task<RealDebridUser> user();

    /// Adds a magnet to the user's RD torrents. The returned id is used
    /// for the rest of the resolution workflow.
    virtual QCoro::Task<RdAddMagnetResult> addMagnet(QString magnet);

    /// Returns the current state of an RD torrent, including its file
    /// list and the per-file hoster links (when available).
    virtual QCoro::Task<RdTorrentInfo> torrentInfo(QString rdTorrentId);

    /// Tells RD which files inside a torrent the user wants. Pass
    /// 1-indexed file ids — the same scheme RD's `torrentInfo` uses.
    /// Pass an empty list to select all files.
    virtual QCoro::Task<void> selectFiles(QString rdTorrentId,
        QList<int> fileIds);

    /// Resolves a hoster link returned by RD into a streamable URL.
    virtual QCoro::Task<RdUnrestrictedLink> unrestrictLink(QUrl link);

    /// Removes the torrent from the user's RD account. Best-effort:
    /// callers typically swallow errors on cleanup.
    virtual QCoro::Task<void> deleteTorrent(QString rdTorrentId);

private:
    core::HttpClient* m_http;
    QUrl m_baseUrl;
    QString m_token;
};

} // namespace kinema::api
