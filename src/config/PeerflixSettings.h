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
 * Peerflix is a zero-configuration public Stremio addon hosted at
 * `https://peerflix.mov`. The only user-facing knob is an optional
 * Base URL override (for self-hosted instances or community mirrors).
 *
 * KConfig group: [Peerflix]
 * Keys:
 *   baseUrl  URL, default "https://peerflix.mov"
 */
class PeerflixSettings : public QObject
{
    Q_OBJECT
public:
    explicit PeerflixSettings(KSharedConfigPtr config,
        QObject* parent = nullptr);

    QString baseUrl() const;
    void setBaseUrl(const QString& url);

    /// Default base URL used when no override has been saved.
    static QString defaultBaseUrl();

Q_SIGNALS:
    void baseUrlChanged(const QString&);

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
