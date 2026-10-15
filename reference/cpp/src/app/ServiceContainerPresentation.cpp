// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/alldebrid/AllDebridClient.h"
#include "api/cinemeta/CinemetaClient.h"
#include "api/indexers/IndexerSelector.h"
#include "api/indexers/PeerflixIndexer.h"
#include "api/indexers/TorrentioIndexer.h"
#include "api/opensubtitles/OpenSubtitlesClient.h"
#include "api/realdebrid/RealDebridClient.h"
#include "api/tmdb/TmdbClient.h"
#include "app/ServiceContainer.h"
#include "config/AppSettings.h"
#include "config/DebridSettings.h"
#include "controllers/DebridCredentialsResolver.h"
#include "controllers/DownloadController.h"
#include "controllers/LibraryController.h"
#ifdef KINEMA_HAVE_LIBMPV
#include "playback/desktop/MprisPlaybackProjection.h"

#endif
#include "controllers/SubtitleController.h"
#include "controllers/TokenController.h"
#include "controllers/WatchedController.h"
#include "core/io/HttpClient.h"
#include "core/mpv/PlayerLauncher.h"
#include "core/persistence/Database.h"
#include "core/persistence/DownloadStore.h"
#include "core/persistence/HistoryStore.h"
#include "core/persistence/LibraryStore.h"
#include "core/persistence/MediaCache.h"
#include "core/persistence/SubtitleCacheStore.h"
#include "core/persistence/TokenStore.h"
#include "core/persistence/TorrentCache.h"
#include "core/persistence/WatchedStore.h"
#include "domain/Debrid.h"
#include "kinema_log_app.h"
#include "playback/adapters/ActiveStreamIndexerAdapter.h"
#include "playback/adapters/ExternalPlayerAdapter.h"
#ifdef KINEMA_HAVE_LIBMPV
#include "ui/player/EmbeddedMpvPlayerAdapter.h"
#endif
#include "playback/downloads/SqliteDownloadRepository.h"
#include "playback/events/PlaybackEventStream.h"
#include "playback/history/HistoryQueryService.h"
#include "playback/history/SqlitePlaybackHistoryRepository.h"
#include "playback/history/TrackMemoryService.h"
#include "playback/progress/PlaybackProgressProjector.h"
#include "playback/resume/ResumeUseCase.h"
#include "playback/series/SeriesSessionService.h"
#include "playback/session/PlaybackSessionManager.h"
#include "playback/sources/AllDebridMediaSource.h"
#include "playback/sources/AllDebridResolver.h"
#include "playback/sources/RealDebridMediaSource.h"
#include "playback/sources/RealDebridResolver.h"
#include "playback/sources/TorrentMediaSource.h"
#include "playback/streaming/LocalHttpStreamGateway.h"
#include "playback/subtitles/MoviehashProbe.h"
#include "playback/subtitles/SubtitleSessionService.h"
#include "playback/torrent/LibtorrentClient.h"
#include "playback/transfer/BackendRegistry.h"
#include "playback/transfer/SessionRegistry.h"
#include "playback/transfer/TransferSupervisor.h"
#include "playback/transfer/TransferUseCase.h"
#include "services/StreamActions.h"
#include "ui/ImageLoader.h"
#include "ui/qml-bridge/browse/BrowseViewModel.h"
#include "ui/qml-bridge/details/MovieDetailViewModel.h"
#include "ui/qml-bridge/details/SeriesDetailViewModel.h"
#include "ui/qml-bridge/discover/DiscoverViewModel.h"
#include "ui/qml-bridge/downloads/DownloadsViewModel.h"
#include "ui/qml-bridge/library/ContinueWatchingViewModel.h"
#include "ui/qml-bridge/library/LibraryViewModel.h"
#include "ui/qml-bridge/search/SearchViewModel.h"
#include "ui/qml-bridge/settings/SettingsRootViewModel.h"
#include "ui/qml-bridge/shell/AppIconResolver.h"
#include "ui/qml-bridge/shell/ShellDependencies.h"
#include "ui/qml-bridge/subtitles/SubtitlesViewModel.h"

#include <QMetaObject>
#include <QObject>
#include <QString>
#include <QTimer>

namespace kinema::app {

void ServiceContainer::buildControllersAndViewModels()
{
    QObject* a = m_anchor.get();

    // OpenSubtitles + subtitle controller. Token / credential
    // routing back to the live `TokenController` is set up in
    // `wirePresentation()`.
    m_openSubtitles = new api::OpenSubtitlesClient(m_http.get(),
                                                   m_tokenCtrl->openSubtitlesApiKey(),
                                                   m_tokenCtrl->openSubtitlesUsername(),
                                                   m_tokenCtrl->openSubtitlesPassword(),
                                                   a);
    m_subtitleCtrl = new controllers::SubtitleController(
        m_openSubtitles, m_subtitleCache.get(), m_settings.subtitle(), m_settings.cache(), a);
    QTimer::singleShot(0, m_subtitleCtrl, [this] { m_subtitleCtrl->reconcileCacheOnStartup(); });

    if (m_subtitleCtrl) {
        // The session service subscribes to PlaybackRequested
        // (clearMoviehash) and MoviehashComputed (setMoviehash),
        // replacing the direct PlaybackController <-> subtitle
        // signal wiring that lived here before.
        m_subtitleSessionService = new playback::subtitles::SubtitleSessionService(
            *m_subtitleCtrl, m_playbackEventStream, a);
    }

    m_libraryCtrl = new controllers::LibraryController(*m_library, m_cinemeta, a);
    // Lazy backfill of v7 schema columns (genres / rating / runtime
    // / cast) for titles saved before that migration. Queued so the
    // first event-loop tick boots the UI cleanly; backfill itself is
    // capped + silent on failure.
    QMetaObject::invokeMethod(
        m_libraryCtrl, &controllers::LibraryController::backfillMetadata, Qt::QueuedConnection);
    m_watchedCtrl = new controllers::WatchedController(*m_watched, m_historyQueryService, a);

    // Downloads page VM. Lives over the entire app lifetime so the
    // drawer's downloads entry can show counts even before the
    // first navigation to the page.
    m_downloadsVm = new ui::qml::DownloadsViewModel(*m_downloadCtrl, m_playbackSessionManager, a);

    // Discover / Search / Browse surface VMs. They sit on top of
    // the existing service graph; action signals route back into
    // `ShellViewModel` either for direct controller forwarding
    // (resume / remove) or for navigation events the QML shell
    // listens for.
    m_discoverVm = new ui::qml::DiscoverViewModel(m_tmdb, m_tokenCtrl, a);
    m_discoverVm->setLibraryController(m_libraryCtrl);
    m_discoverVm->setWatchedController(m_watchedCtrl);
    m_continueWatchingVm = new ui::qml::ContinueWatchingViewModel(m_historyQueryService, a);
    m_libraryVm = new ui::qml::LibraryViewModel(m_libraryCtrl, m_watchedCtrl, a);
    m_searchVm = new ui::qml::SearchViewModel(m_cinemeta, m_tmdb, m_settings.search(), a);
    m_searchVm->setLibraryController(m_libraryCtrl);
    m_searchVm->setWatchedController(m_watchedCtrl);
    m_browseVm = new ui::qml::BrowseViewModel(m_tmdb, m_settings.browse(), a);
    m_browseVm->setLibraryController(m_libraryCtrl);
    m_browseVm->setWatchedController(m_watchedCtrl);
    m_movieDetailVm = new ui::qml::MovieDetailViewModel(m_cinemeta,
                                                        m_indexers,
                                                        m_tmdb,
                                                        m_playbackSessionManager,
                                                        m_streamActions,
                                                        m_libraryCtrl,
                                                        m_watchedCtrl,
                                                        m_tokenCtrl,
                                                        m_settings,
                                                        m_tokenCtrl->realDebridToken(),
                                                        m_tokenCtrl->allDebridApiKey(),
                                                        a);
    m_movieDetailVm->setDownloadController(m_downloadCtrl);
    m_seriesDetailVm = new ui::qml::SeriesDetailViewModel(m_cinemeta,
                                                          m_indexers,
                                                          m_tmdb,
                                                          m_playbackSessionManager,
                                                          m_streamActions,
                                                          m_libraryCtrl,
                                                          m_watchedCtrl,
                                                          m_tokenCtrl,
                                                          m_settings,
                                                          m_tokenCtrl->realDebridToken(),
                                                          m_tokenCtrl->allDebridApiKey(),
                                                          a);
    m_seriesDetailVm->setDownloadController(m_downloadCtrl);

    // Subtitles VM. Wraps `SubtitleController`. Cross-shell
    // routing (settings request, close, attach-to-player) is wired
    // in `ShellViewModel`.
    m_subtitlesVm = new ui::qml::SubtitlesViewModel(m_subtitleCtrl, m_settings.subtitle(), a);

    // Settings root. Token routing back to the live
    // `TokenController` is set up in `wirePresentation()`.
    m_settingsVm = new ui::qml::settings::SettingsRootViewModel(m_http.get(),
                                                                m_tokens.get(),
                                                                m_indexers,
                                                                m_settings,
                                                                m_subtitleCache.get(),
                                                                m_mediaCache.get(),
                                                                m_torrentCache.get(),
                                                                m_downloadCtrl,
                                                                m_imageLoader,
                                                                a);
}

void ServiceContainer::wirePresentation()
{
    QObject* a = m_anchor.get();

    // ---- TokenController ↔ RD / AD clients ----------------------
    // Keep the RD / AD clients' in-memory tokens in sync with the
    // keyring-backed `TokenController`. Both backends check
    // `client.token()` / `client.apiKey()` for `canHandle`; the
    // active-provider gate lives in `BackendRegistry`.
    m_rd->setToken(m_tokenCtrl->realDebridToken());
    m_ad->setApiKey(m_tokenCtrl->allDebridApiKey());
    QObject::connect(m_tokenCtrl,
                     &controllers::TokenController::realDebridTokenChanged,
                     m_rd.get(),
                     [this](const QString& tok) { m_rd->setToken(tok); });
    QObject::connect(m_tokenCtrl,
                     &controllers::TokenController::allDebridApiKeyChanged,
                     m_ad.get(),
                     [this](const QString& key) { m_ad->setApiKey(key); });

    // Forward the active-debrid-provider radio to the backend
    // registry. Initial value + future changes both go through the
    // same path.
    m_backendRegistry->setActiveDebridProvider(m_settings.debrid().activeProvider());
    QObject::connect(
        &m_settings.debrid(),
        &config::DebridSettings::activeProviderChanged,
        a,
        [this](domain::DebridProvider p) { m_backendRegistry->setActiveDebridProvider(p); });

    // ---- TokenController ↔ OpenSubtitles -----------------------
    // Credentials change → drop JWT so the next request re-logs in,
    // and tell the controller to re-evaluate downloadEnabled.
    const auto onOsCredentialChanged = [this](const QString&) {
        m_openSubtitles->clearJwt();
        m_subtitleCtrl->notifyAuthChanged();
    };
    QObject::connect(m_tokenCtrl,
                     &controllers::TokenController::openSubtitlesApiKeyChanged,
                     m_openSubtitles,
                     onOsCredentialChanged);
    QObject::connect(m_tokenCtrl,
                     &controllers::TokenController::openSubtitlesUsernameChanged,
                     m_openSubtitles,
                     onOsCredentialChanged);
    QObject::connect(m_tokenCtrl,
                     &controllers::TokenController::openSubtitlesPasswordChanged,
                     m_openSubtitles,
                     onOsCredentialChanged);

    // ---- TMDB token ↔ Browse VM --------------------------------
    // TMDB token gain/loss propagates from `TokenController` to
    // the Browse VM. We refresh on token gain and clear on loss;
    // the Browse VM's own `tmdbConfigured` property toggles the
    // placeholder visibility from QML.
    QObject::connect(m_tokenCtrl,
                     &controllers::TokenController::tmdbTokenChanged,
                     m_browseVm,
                     [this](const QString& token) {
                         if (token.isEmpty()) {
                             m_browseVm->results()->setItems({});
                         } else {
                             m_browseVm->refresh();
                         }
                     });

    // ---- Settings VM ↔ TokenController -------------------------
    // RD / TMDB / OS credential changes refresh their respective
    // in-memory aliases without a keyring round-trip.
    QObject::connect(m_settingsVm,
                     &ui::qml::settings::SettingsRootViewModel::tmdbTokenChanged,
                     m_tokenCtrl,
                     [this](const QString&) { m_tokenCtrl->refreshTmdb(); });
    QObject::connect(m_settingsVm,
                     &ui::qml::settings::SettingsRootViewModel::realDebridTokenChanged,
                     m_tokenCtrl,
                     [this](const QString&) { m_tokenCtrl->refreshRealDebrid(); });
    QObject::connect(m_settingsVm,
                     &ui::qml::settings::SettingsRootViewModel::allDebridApiKeyChanged,
                     m_tokenCtrl,
                     [this](const QString&) { m_tokenCtrl->refreshAllDebrid(); });
    QObject::connect(m_settingsVm,
                     &ui::qml::settings::SettingsRootViewModel::activeDebridProviderChanged,
                     m_tokenCtrl,
                     [this] {
                         // Pure UX: refresh both tokens so the in-memory copies
                         // re-read the keyring (cheap; tokens didn't change but
                         // it keeps the contract symmetric with the radio).
                         m_tokenCtrl->refreshRealDebrid();
                         m_tokenCtrl->refreshAllDebrid();
                     });
    QObject::connect(m_settingsVm,
                     &ui::qml::settings::SettingsRootViewModel::subtitleCredentialsChanged,
                     m_tokenCtrl,
                     [this] { m_tokenCtrl->refreshOpenSubtitlesCredentials(); });

    // ---- Final boot checks --------------------------------------
    if (!m_player->preferredPlayerAvailable()) {
        qCInfo(KINEMA_APP) << "preferred media player not found on $PATH";
    }
}

} // namespace kinema::app
