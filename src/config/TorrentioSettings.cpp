// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/TorrentioSettings.h"

#include "config/ConfigAccess.h"

namespace kinema::config {

namespace {
constexpr auto kGroup = "Torrentio";
constexpr auto kKeyDefaultSort = "defaultSort";
constexpr auto kKeyBaseUrl = "baseUrl";
} // namespace

QString TorrentioSettings::defaultBaseUrl()
{
    return QStringLiteral("https://torrentio.strem.fun");
}

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

QString TorrentioSettings::baseUrl() const
{
    const auto raw = detail::read(m_config, kGroup, kKeyBaseUrl,
        defaultBaseUrl());
    return raw.isEmpty() ? defaultBaseUrl() : raw;
}

void TorrentioSettings::setBaseUrl(const QString& url)
{
    const auto trimmed = url.trimmed();
    const auto effective = trimmed.isEmpty() ? defaultBaseUrl() : trimmed;
    if (baseUrl() == effective) {
        return;
    }
    detail::write(m_config, kGroup, kKeyBaseUrl, effective);
    Q_EMIT baseUrlChanged(effective);
}

} // namespace kinema::config
