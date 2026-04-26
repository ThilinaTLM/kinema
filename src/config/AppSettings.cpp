// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/AppSettings.h"

namespace kinema::config {

AppSettings::AppSettings(QObject* parent)
    : AppSettings(KSharedConfig::openConfig(), parent)
{
}

AppSettings::AppSettings(KSharedConfigPtr config, QObject* parent)
    : QObject(parent)
    , m_search(config, this)
    , m_player(config, this)
    , m_filter(config, this)
    , m_browse(config, this)
    , m_torrentio(config, this)
    , m_appearance(config, this)
    , m_realDebrid(config, this)
    , m_subtitle(config, this)
    , m_cache(config, this)
{
    connectAggregateSignals();
}

void AppSettings::connectAggregateSignals()
{
    // torrentioOptions() reads defaultSort + excludedResolutions +
    // excludedCategories. Any of those changing should refetch.
    connect(&m_torrentio, &TorrentioSettings::defaultSortChanged,
        this, [this] { Q_EMIT torrentioOptionsChanged(); });
    connect(&m_filter, &FilterSettings::exclusionsChanged,
        this, [this] { Q_EMIT torrentioOptionsChanged(); });
}

core::torrentio::ConfigOptions AppSettings::torrentioOptions() const
{
    core::torrentio::ConfigOptions opts;
    opts.sort = m_torrentio.defaultSort();
    opts.excludedResolutions = m_filter.excludedResolutions();
    opts.excludedCategories = m_filter.excludedCategories();
    return opts;
}

} // namespace kinema::config
