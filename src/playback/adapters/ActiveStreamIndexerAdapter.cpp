// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/adapters/ActiveStreamIndexerAdapter.h"

#include "api/IndexerSelector.h"
#include "domain/Indexer.h"

namespace kinema::playback::adapters {

ActiveStreamIndexerAdapter::ActiveStreamIndexerAdapter(
    api::IndexerSelector* selector)
    : m_selector(selector)
{
}

QCoro::Task<QList<domain::Stream>>
ActiveStreamIndexerAdapter::streamsFor(domain::MediaKind kind,
    const QString& streamId)
{
    if (!m_selector) {
        co_return QList<domain::Stream> {};
    }
    auto* active = m_selector->active();
    if (!active) {
        co_return QList<domain::Stream> {};
    }
    co_return co_await active->streams(kind, streamId);
}

} // namespace kinema::playback::adapters
