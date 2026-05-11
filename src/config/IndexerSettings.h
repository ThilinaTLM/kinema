// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Indexer.h"

#include <KSharedConfig>

#include <QObject>

namespace kinema::config {

/**
 * Persisted state for the active stream indexer.
 *
 * Per-indexer config (base URL, sort, …) lives in each concrete
 * settings class (`TorrentioSettings`, `PeerflixSettings`); this
 * class only owns the active-indexer radio. Both built-in indexers
 * have working zero-config defaults, so there are no "configured"
 * flags to gate the UI.
 *
 * KConfig group: [Indexers]
 * Keys:
 *   active   string ("torrentio" / "peerflix")
 */
class IndexerSettings : public QObject
{
    Q_OBJECT
public:
    explicit IndexerSettings(KSharedConfigPtr config,
        QObject* parent = nullptr);

    domain::IndexerKind activeIndexer() const;
    void setActiveIndexer(domain::IndexerKind k);

Q_SIGNALS:
    void activeIndexerChanged(domain::IndexerKind);

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
