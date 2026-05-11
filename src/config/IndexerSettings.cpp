// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/IndexerSettings.h"

#include "config/ConfigAccess.h"

namespace kinema::config {

namespace {
constexpr auto kGroup = "Indexers";
constexpr auto kKeyActive = "active";
constexpr auto kKeyTorrentioConfigured = "torrentioConfigured";
constexpr auto kKeyMediaFusionConfigured = "mediaFusionConfigured";
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

bool IndexerSettings::torrentioConfigured() const
{
    // Torrentio works without user config (public host, no auth) so
    // we default to true. The settings page still lets users tweak
    // base URL / default sort, but those are optional.
    return detail::read(m_config, kGroup, kKeyTorrentioConfigured, true);
}

void IndexerSettings::setTorrentioConfigured(bool on)
{
    if (torrentioConfigured() == on) {
        return;
    }
    detail::write(m_config, kGroup, kKeyTorrentioConfigured, on);
    Q_EMIT torrentioConfiguredChanged(on);
}

bool IndexerSettings::mediaFusionConfigured() const
{
    return detail::read(m_config, kGroup, kKeyMediaFusionConfigured, false);
}

void IndexerSettings::setMediaFusionConfigured(bool on)
{
    if (mediaFusionConfigured() == on) {
        return;
    }
    detail::write(m_config, kGroup, kKeyMediaFusionConfigured, on);
    Q_EMIT mediaFusionConfiguredChanged(on);
}

} // namespace kinema::config
