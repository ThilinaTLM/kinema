// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <KSharedConfig>

#include <QObject>
#include <QString>

namespace kinema::config {

/**
 * Peerflix per-user settings.
 *
 * Peerflix is published as two upstream hosts:
 *   - `https://peerflix.mov` is an IPFS-hosted zero-configuration
 *     mirror. No `/<config>/` path segment is meaningful there.
 *   - `https://addon.peerflix.mov` is the configurable Node service
 *     that accepts a pipe-separated path segment (the same shape
 *     Torrentio uses) carrying debrid credentials and tuning knobs.
 *
 * The Peerflix indexer hits `baseUrl` when no debrid provider is
 * active and `addonBaseUrl` when one is. Both are independently
 * user-overridable so self-hosted mirrors can override one or both.
 *
 * KConfig group: [Peerflix]
 * Keys:
 *   baseUrl       URL, default "https://peerflix.mov"
 *   addonBaseUrl  URL, default "https://addon.peerflix.mov"
 */
class PeerflixSettings : public QObject
{
    Q_OBJECT
public:
    explicit PeerflixSettings(KSharedConfigPtr config,
        QObject* parent = nullptr);

    QString baseUrl() const;
    void setBaseUrl(const QString& url);

    QString addonBaseUrl() const;
    void setAddonBaseUrl(const QString& url);

    /// Default base URL used when no override has been saved.
    static QString defaultBaseUrl();

    /// Default addon base URL used when no override has been saved.
    static QString defaultAddonBaseUrl();

Q_SIGNALS:
    void baseUrlChanged(const QString&);
    void addonBaseUrlChanged(const QString&);

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
