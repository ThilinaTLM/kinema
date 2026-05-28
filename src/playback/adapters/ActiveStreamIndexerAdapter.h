// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "playback/ports/StreamIndexerPort.h"

namespace kinema::api {
class IndexerSelector;
}

namespace kinema::playback::adapters {

/// Adapter that forwards `StreamIndexerPort::streamsFor` to the
/// currently-active indexer via `api::IndexerSelector::active()`.
class ActiveStreamIndexerAdapter final : public ports::StreamIndexerPort
{
public:
    explicit ActiveStreamIndexerAdapter(api::IndexerSelector* selector);
    ~ActiveStreamIndexerAdapter() override = default;

    QCoro::Task<QList<domain::Stream>> streamsFor(
        domain::MediaKind kind, const QString& streamId) override;

private:
    api::IndexerSelector* m_selector;
};

} // namespace kinema::playback::adapters
