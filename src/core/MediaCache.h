// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QDir>
#include <QHash>
#include <QObject>
#include <QSet>
#include <QString>

namespace kinema::config {
class DownloadSettings;
}

namespace kinema::core {

/**
 * Filesystem-backed cache for the unified downloader. Generalises the
 * previous torrent-only `TorrentCache`: every download backend
 * (torrent or RD HTTP) stores its payload + sidecar state under
 * `cache::mediaDir()/<assetId>/`.
 *
 * `MediaCache` knows three things about an asset:
 *
 *  - **active**: a session is currently using it (playback or
 *    background prefetch). Active assets are never evicted.
 *  - **pinned**: marked by the user as an explicit download. Pinned
 *    assets are never auto-evicted, even when inactive.
 *  - **ephemeral**: opportunistic playback cache. Evictable LRU once
 *    inactive.
 *
 * Stamps are tracked via a `.last-used` marker file in the asset
 * directory, identical in shape to the torrent cache. Pinned status
 * is materialised as a `.pinned` marker file so it survives restarts
 * even before the `DownloadStore` row is loaded.
 */
class MediaCache : public QObject
{
    Q_OBJECT
public:
    explicit MediaCache(const config::DownloadSettings& settings,
        QObject* parent = nullptr);

    /// `cache::mediaDir()`.
    QDir rootDir() const;

    /// `<root>/<assetId>/`. Created on first call.
    QDir assetDir(const QString& assetId) const;

    /// Sidecar paths inside the asset directory.
    QString accessMarkerPath(const QString& assetId) const;
    QString pinnedMarkerPath(const QString& assetId) const;

    /// Active = "currently in use; do not evict".
    void markActive(const QString& assetId);
    void markInactive(const QString& assetId);
    bool isActive(const QString& assetId) const;

    /// Pinned = "user-requested download; never auto-evict".
    void setPinned(const QString& assetId, bool on);
    bool isPinned(const QString& assetId) const;

    /// Updates the `.last-used` marker. No-op when the asset dir does
    /// not yet exist.
    void touch(const QString& assetId) const;

    /// Recursively delete an asset directory.
    bool removeAsset(const QString& assetId);

    /// Total bytes used across the entire media cache.
    qint64 sizeBytes() const;

    /// Bytes used by ephemeral (non-pinned) assets only. Useful for
    /// the budget UI: pinned bytes are not counted against the
    /// configurable budget.
    qint64 ephemeralSizeBytes() const;

    /// Configured budget in bytes (`DownloadSettings::cacheBudgetGb`).
    qint64 budgetBytes() const;

public Q_SLOTS:
    /// Remove ephemeral, inactive assets LRU-first until total
    /// ephemeral usage is at or below the budget.
    void enforceBudget();

private:
    static QString normalizedAssetId(QString id);
    bool hasPinnedMarker(const QString& assetId) const;
    qint64 directorySize(const QString& path) const;

    const config::DownloadSettings& m_settings;
    QSet<QString> m_active;
};

} // namespace kinema::core
