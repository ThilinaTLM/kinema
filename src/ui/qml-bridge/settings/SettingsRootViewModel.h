// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>
#include <QString>
#include "ui/qml-bridge/settings/DebridSettingsViewModel.h"
#include "ui/qml-bridge/settings/GeneralSettingsViewModel.h"
#include "ui/qml-bridge/settings/IndexerSettingsViewModel.h"
#include "ui/qml-bridge/settings/PlayerSettingsViewModel.h"
#include "ui/qml-bridge/settings/StreamsSettingsViewModel.h"
#include "ui/qml-bridge/settings/SubtitlesSettingsViewModel.h"
#include "ui/qml-bridge/settings/TmdbSettingsViewModel.h"
#include "ui/qml-bridge/settings/TorrentStreamingSettingsViewModel.h"

namespace kinema::api {
class IndexerSelector;
}

namespace kinema::config {
class AppSettings;
}

namespace kinema::core {
class HttpClient;
class MediaCache;
class SubtitleCacheStore;
class TokenStore;
}

namespace kinema::ui::qml::settings {

class SettingsRootViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(GeneralSettingsViewModel* general READ general CONSTANT)
    Q_PROPERTY(TmdbSettingsViewModel* tmdb READ tmdb CONSTANT)
    Q_PROPERTY(DebridSettingsViewModel* debrid READ debrid CONSTANT)
    Q_PROPERTY(IndexerSettingsViewModel* indexers READ indexers CONSTANT)
    Q_PROPERTY(StreamsSettingsViewModel* streams READ streams CONSTANT)
    Q_PROPERTY(PlayerSettingsViewModel* player READ player CONSTANT)
    Q_PROPERTY(SubtitlesSettingsViewModel* subtitles READ subtitles CONSTANT)
    Q_PROPERTY(TorrentStreamingSettingsViewModel* torrentStreaming READ torrentStreaming CONSTANT)

public:
    SettingsRootViewModel(core::HttpClient* http,
        core::TokenStore* tokens,
        api::IndexerSelector* indexers,
        config::AppSettings& settings,
        core::SubtitleCacheStore* subtitleCache,
        core::MediaCache* mediaCache,
        QObject* parent = nullptr);

    GeneralSettingsViewModel* general() const { return m_general; }
    TmdbSettingsViewModel* tmdb() const { return m_tmdb; }
    DebridSettingsViewModel* debrid() const { return m_debrid; }
    IndexerSettingsViewModel* indexers() const { return m_indexers; }
    StreamsSettingsViewModel* streams() const { return m_streams; }
    PlayerSettingsViewModel* player() const { return m_player; }
    SubtitlesSettingsViewModel* subtitles() const { return m_subs; }
    TorrentStreamingSettingsViewModel* torrentStreaming() const { return m_torrentStreaming; }

Q_SIGNALS:
    /// Forwarded from `TmdbSettingsViewModel::tokenChanged`.
    void tmdbTokenChanged(const QString& token);
    /// Forwarded from `RealDebridSectionViewModel::tokenChanged`.
    void realDebridTokenChanged(const QString& token);
    /// Forwarded from `AllDebridSectionViewModel::apiKeyChanged`.
    void allDebridApiKeyChanged(const QString& apiKey);
    /// Forwarded from `DebridSettingsViewModel::activeProviderChanged`.
    void activeDebridProviderChanged();
    /// Forwarded from `SubtitlesSettingsViewModel::credentialsChanged`.
    void subtitleCredentialsChanged();

private:
    GeneralSettingsViewModel* m_general {};
    TmdbSettingsViewModel* m_tmdb {};
    DebridSettingsViewModel* m_debrid {};
    IndexerSettingsViewModel* m_indexers {};
    StreamsSettingsViewModel* m_streams {};
    PlayerSettingsViewModel* m_player {};
    SubtitlesSettingsViewModel* m_subs {};
    TorrentStreamingSettingsViewModel* m_torrentStreaming {};
};


} // namespace kinema::ui::qml::settings
