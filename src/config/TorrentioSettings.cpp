// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/TorrentioSettings.h"

#include <KConfigGroup>

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
    const auto s = m_config->group(QString::fromLatin1(kGroup))
                       .readEntry(kKeyDefaultSort, QStringLiteral("seeders"));
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
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kKeyDefaultSort, core::torrentio::toString(m));
    g.sync();
    Q_EMIT defaultSortChanged(m);
}

bool TorrentioSettings::cachedOnly() const
{
    return m_config->group(QString::fromLatin1(kGroup))
        .readEntry(kKeyCachedOnly, false);
}

void TorrentioSettings::setCachedOnly(bool on)
{
    if (cachedOnly() == on) {
        return;
    }
    auto g = m_config->group(QString::fromLatin1(kGroup));
    g.writeEntry(kKeyCachedOnly, on);
    g.sync();
    Q_EMIT cachedOnlyChanged(on);
}

} // namespace kinema::config
