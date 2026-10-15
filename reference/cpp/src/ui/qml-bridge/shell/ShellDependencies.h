// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

namespace kinema::config {
class AppSettings;
}
namespace kinema::controllers {
class DownloadController;
class LibraryController;
class SubtitleController;
class TokenController;
class TrayController;
} // namespace kinema::controllers
namespace kinema::core {
class PlayerLauncher;
}
namespace kinema::playback::events {
class PlaybackEventStream;
}
namespace kinema::playback::resume {
class ResumeUseCase;
}
namespace kinema::playback::session {
class PlaybackSessionManager;
}
namespace kinema::playback::torrent {
class LibtorrentClient;
}
#ifdef KINEMA_HAVE_LIBMPV
namespace kinema::playback::desktop {
class MprisPlaybackProjection;
}
namespace kinema::playback::series {
class SeriesSessionService;
}
namespace kinema::ui::player {
class EmbeddedMpvPlayerAdapter;
}
#endif
namespace kinema::services {
class StreamActions;
}
namespace kinema::ui::qml {
class BrowseViewModel;
class ContinueWatchingViewModel;
class DiscoverViewModel;
class DownloadsViewModel;
class LibraryViewModel;
class MovieDetailViewModel;
class SearchViewModel;
class SeriesDetailViewModel;
class SubtitlesViewModel;

/** Explicit, non-owning inputs used by the QML shell coordinator. */
struct ShellDependencies
{
    config::AppSettings& settings;
    core::PlayerLauncher* player;
    services::StreamActions* streamActions;
    controllers::DownloadController* downloadController;
    controllers::LibraryController* libraryController;
    controllers::SubtitleController* subtitleController;
    controllers::TokenController* tokenController;
    controllers::TrayController*& tray;
    playback::events::PlaybackEventStream* playbackEvents;
    playback::resume::ResumeUseCase* resume;
    playback::session::PlaybackSessionManager* playbackSessions;
    playback::torrent::LibtorrentClient* libtorrent;
    BrowseViewModel* browse;
    ContinueWatchingViewModel* continueWatching;
    DiscoverViewModel* discover;
    DownloadsViewModel* downloads;
    LibraryViewModel* library;
    MovieDetailViewModel* movieDetail;
    SearchViewModel* search;
    SeriesDetailViewModel* seriesDetail;
    SubtitlesViewModel* subtitles;
#ifdef KINEMA_HAVE_LIBMPV
    playback::desktop::MprisPlaybackProjection* mpris;
    playback::series::SeriesSessionService* seriesSession;
    ui::player::EmbeddedMpvPlayerAdapter* embeddedPlayer;
#endif
};

} // namespace kinema::ui::qml
