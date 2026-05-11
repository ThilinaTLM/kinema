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
    , m_library(config, this)
    , m_torrentio(config, this)
    , m_appearance(config, this)
    , m_debrid(config, this)
    , m_subtitle(config, this)
    , m_cache(config, this)
    , m_torrentStreaming(config, this)
    , m_download(config, this)
    , m_indexers(config, this)
    , m_mediaFusion(config, this)
{
}

} // namespace kinema::config
