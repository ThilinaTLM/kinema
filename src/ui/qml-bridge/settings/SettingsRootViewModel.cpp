// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/settings/SettingsRootViewModel.h"
#include "api/IndexerSelector.h"
#include "config/AppSettings.h"
#include "core/io/HttpClient.h"
#include "core/persistence/MediaCache.h"
#include "core/persistence/SubtitleCacheStore.h"
#include "core/persistence/TokenStore.h"

namespace kinema::ui::qml::settings {

// ============================== Root ======================================

SettingsRootViewModel::SettingsRootViewModel(core::HttpClient* http,
    core::TokenStore* tokens, api::IndexerSelector* indexers,
    config::AppSettings& settings,
    core::SubtitleCacheStore* subtitleCache,
    core::MediaCache* mediaCache, QObject* parent)
    : QObject(parent)
{
    Q_ASSERT(mediaCache);
    m_general = new GeneralSettingsViewModel(settings.search(),
        settings.appearance(), this);
    m_tmdb = new TmdbSettingsViewModel(http, tokens, this);
    m_debrid = new DebridSettingsViewModel(http, tokens,
        settings.debrid(), this);
    m_indexers = new IndexerSettingsViewModel(indexers,
        settings.indexers(), settings.torrentio(),
        settings.peerflix(), this);
    m_streams = new StreamsSettingsViewModel(settings.filter(), this);
    m_player = new PlayerSettingsViewModel(settings.player(), this);
    m_subs = new SubtitlesSettingsViewModel(http, tokens,
        settings.subtitle(), settings.cache(), subtitleCache, this);
    m_torrentStreaming = new TorrentStreamingSettingsViewModel(
        settings.torrentStreaming(), *mediaCache, this);

    // Forward token / credential changes through the root so
    // `MainController` can route them to `TokenController`.
    connect(m_tmdb, &TmdbSettingsViewModel::tokenChanged, this,
        &SettingsRootViewModel::tmdbTokenChanged);
    connect(m_debrid->realDebrid(),
        &RealDebridSectionViewModel::tokenChanged, this,
        &SettingsRootViewModel::realDebridTokenChanged);
    connect(m_debrid->allDebrid(),
        &AllDebridSectionViewModel::apiKeyChanged, this,
        &SettingsRootViewModel::allDebridApiKeyChanged);
    connect(m_debrid,
        &DebridSettingsViewModel::activeProviderChanged, this,
        &SettingsRootViewModel::activeDebridProviderChanged);
    connect(m_subs,
        &SubtitlesSettingsViewModel::credentialsChanged, this,
        &SettingsRootViewModel::subtitleCredentialsChanged);

    // Initial async loads. Each VM's loadTask is independent and
    // safe to run concurrently — they hit different keyring keys.
    m_tmdb->load();
    m_debrid->realDebrid()->load();
    m_debrid->allDebrid()->load();
    m_subs->load();
}

} // namespace kinema::ui::qml::settings
