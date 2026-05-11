// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/IndexerSettings.h"

#include "config/ConfigAccess.h"

namespace kinema::config {

namespace {
constexpr auto kGroup = "Indexers";
constexpr auto kKeyActive = "active";
} // namespace

IndexerSettings::IndexerSettings(KSharedConfigPtr config, QObject* parent)
    : QObject(parent)
    , m_config(std::move(config))
{
}

api::IndexerKind IndexerSettings::activeIndexer() const
{
    const auto raw = detail::read(m_config, kGroup, kKeyActive,
        QString { QStringLiteral("torrentio") });
    return api::indexerKindFromString(raw);
}

void IndexerSettings::setActiveIndexer(api::IndexerKind k)
{
    if (activeIndexer() == k) {
        return;
    }
    detail::write(m_config, kGroup, kKeyActive,
        api::indexerKindToString(k));
    Q_EMIT activeIndexerChanged(k);
}

} // namespace kinema::config
