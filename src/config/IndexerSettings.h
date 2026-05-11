// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Indexer.h"

#include <KSharedConfig>

#include <QObject>

namespace kinema::config {

/**
 * Persisted state for the active stream indexer.
 *
 * Mirrors `DebridSettings`: a single radio-style "active" choice plus
 * a couple of "is configured?" booleans the settings page uses to
 * gate Test / Save buttons. Per-indexer config (base URL, sort,
 * MediaFusion token, …) lives in each concrete settings class
 * (`TorrentioSettings`, `MediaFusionSettings`) — this class only
 * owns the active-indexer radio and the configured flags.
 *
 * KConfig group: [Indexers]
 * Keys:
 *   active                 string ("torrentio" / "mediafusion")
 *   torrentioConfigured    bool (default true — Torrentio works without setup)
 *   mediaFusionConfigured  bool (default false)
 */
class IndexerSettings : public QObject
{
    Q_OBJECT
public:
    explicit IndexerSettings(KSharedConfigPtr config,
        QObject* parent = nullptr);

    api::IndexerKind activeIndexer() const;
    void setActiveIndexer(api::IndexerKind k);

    bool torrentioConfigured() const;
    void setTorrentioConfigured(bool);

    bool mediaFusionConfigured() const;
    void setMediaFusionConfigured(bool);

Q_SIGNALS:
    void activeIndexerChanged(api::IndexerKind);
    void torrentioConfiguredChanged(bool);
    void mediaFusionConfiguredChanged(bool);

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
