// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/TorrentioSettings.h"

#include "config/ConfigAccess.h"

namespace kinema::config {

namespace {
constexpr auto kGroup = "General";
constexpr auto kKeyDefaultSort = "defaultSort";
constexpr auto kKeyCachedOnly = "cachedOnly";
} // namespace

TorrentioSettings::TorrentioSettings(KSharedConfigPtr config, QObject* parent)
    : QObject(parent)
    , m_config(std::move(config))
{
}

core::torrentio::SortMode TorrentioSettings::defaultSort() const
{
    const auto s = detail::read(m_config, kGroup, kKeyDefaultSort,
        QStringLiteral("seeders"));
    if (s == QLatin1String("size")) {
        return core::torrentio::SortMode::Size;
    }
    if (s == QLatin1String("qualitysize")) {
        return core::torrentio::SortMode::QualitySize;
    }
    return core::torrentio::SortMode::Seeders;
}

void TorrentioSettings::setDefaultSort(core::torrentio::SortMode m)
{
    if (defaultSort() == m) {
        return;
    }
    detail::write(m_config, kGroup, kKeyDefaultSort,
        core::torrentio::toString(m));
    Q_EMIT defaultSortChanged(m);
}

bool TorrentioSettings::cachedOnly() const
{
    return detail::read(m_config, kGroup, kKeyCachedOnly, false);
}

void TorrentioSettings::setCachedOnly(bool on)
{
    if (cachedOnly() == on) {
        return;
    }
    detail::write(m_config, kGroup, kKeyCachedOnly, on);
    Q_EMIT cachedOnlyChanged(on);
}

} // namespace kinema::config
