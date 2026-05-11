// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/IndexerSelector.h"

#include "config/IndexerSettings.h"
#include "kinema_log_api.h"

namespace kinema::api {

IndexerSelector::IndexerSelector(config::IndexerSettings& settings,
    QObject* parent)
    : QObject(parent)
    , m_settings(settings)
{
    connect(&m_settings, &config::IndexerSettings::activeIndexerChanged,
        this, [this](api::IndexerKind k) {
            qCDebug(KINEMA_API) << "IndexerSelector: active indexer ->"
                                << indexerKindToString(k);
            Q_EMIT activeIndexerChanged(k);
        });
}

IndexerSelector::~IndexerSelector() = default;

void IndexerSelector::registerIndexer(std::unique_ptr<Indexer> indexer)
{
    if (!indexer) {
        return;
    }
    const auto k = indexer->kind();
    // Replace any earlier registration for the same kind so callers
    // can swap implementations at runtime (e.g. tests).
    for (auto& slot : m_indexers) {
        if (slot && slot->kind() == k) {
            slot = std::move(indexer);
            return;
        }
    }
    m_indexers.push_back(std::move(indexer));
}

Indexer* IndexerSelector::active() const
{
    return find(m_settings.activeIndexer());
}

Indexer* IndexerSelector::find(IndexerKind k) const
{
    for (const auto& i : m_indexers) {
        if (i && i->kind() == k) {
            return i.get();
        }
    }
    return nullptr;
}

QList<Indexer*> IndexerSelector::all() const
{
    QList<Indexer*> out;
    out.reserve(static_cast<int>(m_indexers.size()));
    for (const auto& i : m_indexers) {
        if (i) {
            out.append(i.get());
        }
    }
    return out;
}

} // namespace kinema::api
