// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Indexer.h"

#include <QList>
#include <QObject>

#include <memory>
#include <vector>

namespace kinema::config {
class IndexerSettings;
}

namespace kinema::api {

/**
 * Owns the registered `Indexer` instances and exposes the currently
 * active one per `IndexerSettings::activeIndexer()`.
 *
 * Mirrors `download::BackendSelector`: a single typed radio
 * (single active) for v1; the interface is shaped so a future
 * `streamsMerged()` can fan out across all registered indexers
 * and dedupe by `infoHash` without touching call sites.
 *
 * View-models / controllers hold an `IndexerSelector*` (constructed
 * in `MainController`) and call `selector->active()->streams(...)`.
 * When the settings page changes `activeIndexer`, the selector
 * re-emits the change so consumers can decide to invalidate
 * cached stream lists.
 */
class IndexerSelector : public QObject
{
    Q_OBJECT
public:
    explicit IndexerSelector(config::IndexerSettings& settings,
        QObject* parent = nullptr);
    ~IndexerSelector() override;

    /// Takes ownership. Registering the same kind twice replaces
    /// the previous registration (last write wins).
    void registerIndexer(std::unique_ptr<Indexer> indexer);

    /// The indexer matching `IndexerSettings::activeIndexer()`.
    /// Returns `nullptr` if no indexer is registered for that kind
    /// (programmer error; callers should surface "no indexer
    /// configured" instead of crashing).
    Indexer* active() const;

    /// Look up by kind regardless of active status. The settings
    /// page Test button uses this to probe the non-active indexer
    /// without flipping the radio.
    Indexer* find(IndexerKind) const;

    /// Snapshot of all registered indexers. Order matches
    /// registration order. Used by the settings page to enumerate
    /// sections.
    QList<Indexer*> all() const;

Q_SIGNALS:
    /// Re-emitted from `IndexerSettings::activeIndexerChanged`.
    void activeIndexerChanged(api::IndexerKind);

private:
    config::IndexerSettings& m_settings;
    std::vector<std::unique_ptr<Indexer>> m_indexers;
};

} // namespace kinema::api
