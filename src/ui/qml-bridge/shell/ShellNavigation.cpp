// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/AppSettings.h"
#include "config/AppearanceSettings.h"
#include "controllers/DownloadController.h"
#include "controllers/LibraryController.h"
#include "playback/resume/ResumeUseCase.h"
#include "playback/session/PlaybackSessionManager.h"
#include "ui/qml-bridge/shell/ShellViewModel.h"
#ifdef KINEMA_HAVE_LIBMPV
#include "playback/desktop/MprisPlaybackProjection.h"
#include "playback/events/PlaybackEventStream.h"
#include "playback/series/SeriesSessionService.h"
#include "ui/player/EmbeddedMpvPlayerAdapter.h"
#endif
#include "controllers/SubtitleController.h"
#include "controllers/TokenController.h"
#include "controllers/TrayController.h"
#include "core/io/OpenUrl.h"
#include "core/mpv/PlayerLauncher.h"
#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "kinema_log_app.h"
#ifdef KINEMA_HAVE_LIBMPV
#include "ui/player/EmbeddedMpvPlayerAdapter.h"
#endif
#include "playback/torrent/LibtorrentClient.h"
#include "services/StreamActions.h"
#include "ui/qml-bridge/browse/BrowseViewModel.h"
#include "ui/qml-bridge/details/MovieDetailViewModel.h"
#include "ui/qml-bridge/details/SeriesDetailViewModel.h"
#include "ui/qml-bridge/discover/DiscoverViewModel.h"
#include "ui/qml-bridge/downloads/DownloadsViewModel.h"
#include "ui/qml-bridge/library/ContinueWatchingViewModel.h"
#include "ui/qml-bridge/library/LibraryViewModel.h"
#include "ui/qml-bridge/search/SearchViewModel.h"
#include "ui/qml-bridge/subtitles/SubtitlesViewModel.h"
#ifdef KINEMA_HAVE_LIBMPV
#include "ui/player/PlayerViewModel.h"
#include "ui/player/PlayerWindow.h"
#endif

#include <QClipboard>
#include <QCoreApplication>
#include <QGuiApplication>
#include <QQuickWindow>
#include <QRegularExpression>
#include <QUrl>

#include <KAboutData>
#include <KLocalizedString>
#include <KNotification>

namespace kinema::ui::qml {

void ShellViewModel::requestSettings(const QString& category)
{
    Q_EMIT showSettingsRequested(category);
}

void ShellViewModel::pushSubtitlesPage(const domain::PlaybackContext& ctx, bool fromPlayer)
{
    auto* vm = m_dependencies.subtitles;
    if (!vm) {
        return;
    }
    vm->setAttachOnDownload(fromPlayer);
    vm->setMedia(ctx);
    Q_EMIT showSubtitlesRequested();
}

void ShellViewModel::requestAbout()
{
    Q_EMIT showAboutRequested();
}

void ShellViewModel::requestFocusSearch()
{
    Q_EMIT focusSearchRequested();
}

void ShellViewModel::applyBrowsePreset(int kind, int sort)
{
    auto* vm = m_dependencies.browse;
    if (!vm) {
        return;
    }
    vm->applyPreset(kind, sort);
    Q_EMIT navigateToBrowseRequested();
}

void ShellViewModel::openMovieDetail(const QString& imdbId, const QString& /*title*/)
{
    auto* vm = m_dependencies.movieDetail;
    if (!vm || imdbId.isEmpty()) {
        return;
    }
    vm->load(imdbId);
    Q_EMIT showMovieDetailRequested();
}

void ShellViewModel::openMovieDetailByTmdb(int tmdbId, const QString& title)
{
    auto* vm = m_dependencies.movieDetail;
    if (!vm || tmdbId <= 0) {
        return;
    }
    vm->loadByTmdbId(tmdbId, title);
    Q_EMIT showMovieDetailRequested();
}

void ShellViewModel::openSeriesDetail(const QString& imdbId, const QString& /*title*/)
{
    auto* vm = m_dependencies.seriesDetail;
    if (!vm || imdbId.isEmpty()) {
        return;
    }
    vm->load(imdbId);
    Q_EMIT showSeriesDetailRequested();
}

void ShellViewModel::openSeriesDetailAt(const QString& imdbId,
                                        const QString& /*title*/,
                                        int season,
                                        int episode)
{
    auto* vm = m_dependencies.seriesDetail;
    if (!vm || imdbId.isEmpty()) {
        return;
    }
    vm->loadAt(imdbId, season, episode);
    Q_EMIT showSeriesDetailRequested();
}

void ShellViewModel::openSeriesDetailByTmdb(int tmdbId, const QString& title)
{
    auto* vm = m_dependencies.seriesDetail;
    if (!vm || tmdbId <= 0) {
        return;
    }
    vm->loadByTmdbId(tmdbId, title);
    Q_EMIT showSeriesDetailRequested();
}

void ShellViewModel::copyToClipboard(const QString& text, const QString& confirmMessage)
{
    if (text.isEmpty()) {
        return;
    }
    QGuiApplication::clipboard()->setText(text);
    const QString toast =
        confirmMessage.isEmpty() ? i18nc("@info:status", "Copied to clipboard") : confirmMessage;
    Q_EMIT passiveMessage(toast, 3000);
}

void ShellViewModel::openExternalUrl(const QUrl& url)
{
    if (!url.isValid() || url.isEmpty()) {
        return;
    }
    core::io::openExternal(url, this, [this](const core::io::OpenExternalResult& r) {
        if (!r.ok) {
            Q_EMIT passiveMessage(i18nc("@info:status", "Could not open link: %1", r.errorString),
                                  6000);
        }
    });
}

void ShellViewModel::openImdbTitle(const QString& imdbId)
{
    static const QRegularExpression kImdbIdRe(QStringLiteral("^tt\\d+$"));
    if (imdbId.isEmpty() || !kImdbIdRe.match(imdbId).hasMatch()) {
        return;
    }
    openExternalUrl(QUrl(QStringLiteral("https://www.imdb.com/title/%1/").arg(imdbId)));
}

void ShellViewModel::openTmdbTitle(int tmdbId, int kind)
{
    if (tmdbId <= 0) {
        return;
    }
    const QString seg = (kind == static_cast<int>(domain::MediaKind::Series))
                            ? QStringLiteral("tv")
                            : QStringLiteral("movie");
    openExternalUrl(QUrl(QStringLiteral("https://www.themoviedb.org/%1/%2").arg(seg).arg(tmdbId)));
}

void ShellViewModel::findMovieStreamsByImdb(const QString& imdbId, const QString& /*title*/)
{
    auto* mv = m_dependencies.movieDetail;
    if (!mv || imdbId.isEmpty()) {
        return;
    }
    mv->clear();
    mv->load(imdbId);
    Q_EMIT showStreamsRequested(mv);
}

void ShellViewModel::findSeriesStreamsByImdb(const QString& imdbId, const QString& /*title*/)
{
    auto* sv = m_dependencies.seriesDetail;
    if (!sv || imdbId.isEmpty()) {
        return;
    }
    sv->clear();
    sv->load(imdbId);
    Q_EMIT showStreamsRequested(sv);
}

void ShellViewModel::findMovieStreamsByTmdb(int tmdbId, const QString& title)
{
    auto* mv = m_dependencies.movieDetail;
    if (!mv || tmdbId <= 0) {
        return;
    }
    mv->clear();
    mv->loadByTmdbId(tmdbId, title);
    Q_EMIT showStreamsRequested(mv);
}

void ShellViewModel::findSeriesStreamsByTmdb(int tmdbId, const QString& title)
{
    auto* sv = m_dependencies.seriesDetail;
    if (!sv || tmdbId <= 0) {
        return;
    }
    sv->clear();
    sv->loadByTmdbId(tmdbId, title);
    Q_EMIT showStreamsRequested(sv);
}

void ShellViewModel::wireNavigationRouting()
{
    auto* continueWatchingVm = m_dependencies.continueWatching;
    auto* resumeUseCase = m_dependencies.resume;
    auto* libraryVm = m_dependencies.library;
    auto* discoverVm = m_dependencies.discover;
    auto* searchVm = m_dependencies.search;
    auto* browseVm = m_dependencies.browse;
    auto* movieDetailVm = m_dependencies.movieDetail;
    auto* seriesDetailVm = m_dependencies.seriesDetail;
    auto* subtitlesVm = m_dependencies.subtitles;

    // Continue Watching action routing. Resume / remove go straight
    // to the history controller; Details pushes the matching detail
    // page; Streams reuses the detail VMs directly and asks the
    // shell to push `StreamsPage` without an intermediate detail-page
    // push. Series entries thread the saved season + episode through
    // the detail VM so both routes land on the remembered episode.
    connect(continueWatchingVm,
            &ContinueWatchingViewModel::resumeRequested,
            resumeUseCase,
            &playback::resume::ResumeUseCase::resume);
    connect(continueWatchingVm,
            &ContinueWatchingViewModel::removeRequested,
            resumeUseCase,
            &playback::resume::ResumeUseCase::removeEntry);
    const auto openHistoryDetail = [this](const domain::HistoryEntry& entry) {
        if (entry.key.kind == domain::MediaKind::Movie) {
            openMovieDetail(entry.key.imdbId, entry.title);
            return;
        }
        const auto title = entry.seriesTitle.isEmpty() ? entry.title : entry.seriesTitle;
        openSeriesDetailAt(
            entry.key.imdbId, title, entry.key.season.value_or(-1), entry.key.episode.value_or(-1));
    };
    const auto openHistoryStreams = [this, openHistoryDetail](const domain::HistoryEntry& entry) {
        if (entry.key.kind == domain::MediaKind::Movie) {
            auto* mv = m_dependencies.movieDetail;
            if (!mv || entry.key.imdbId.isEmpty()) {
                return;
            }
            mv->clear();
            mv->load(entry.key.imdbId);
            Q_EMIT showStreamsRequested(mv);
            return;
        }

        auto* sv = m_dependencies.seriesDetail;
        if (!sv || entry.key.imdbId.isEmpty() || !entry.key.season || !entry.key.episode) {
            openHistoryDetail(entry);
            return;
        }
        sv->clear();
        sv->loadAt(entry.key.imdbId, *entry.key.season, *entry.key.episode);
        Q_EMIT showStreamsRequested(sv);
    };
    connect(
        continueWatchingVm, &ContinueWatchingViewModel::detailRequested, this, openHistoryDetail);
    connect(
        continueWatchingVm, &ContinueWatchingViewModel::streamsRequested, this, openHistoryStreams);
    // Resume-from-history fallback: the saved release is gone, so
    // open the matching detail page so the user can pick another
    // stream.
    connect(resumeUseCase,
            &playback::resume::ResumeUseCase::resumeFallbackRequested,
            this,
            openHistoryDetail);

    connect(libraryVm,
            &LibraryViewModel::resumeRequested,
            resumeUseCase,
            &playback::resume::ResumeUseCase::resume);
    connect(
        libraryVm, &LibraryViewModel::openMovieRequested, this, &ShellViewModel::openMovieDetail);
    connect(
        libraryVm, &LibraryViewModel::openSeriesRequested, this, &ShellViewModel::openSeriesDetail);
    connect(libraryVm,
            &LibraryViewModel::openSeriesEpisodeRequested,
            this,
            &ShellViewModel::openSeriesDetailAt);
    // Resume-rail activations: load the matching detail VM as the
    // streams backing context, then push `StreamsPage` directly.
    // Mirrors the Continue Watching "Streams" action in shape.
    connect(libraryVm,
            &LibraryViewModel::openMovieStreamsRequested,
            this,
            [this](const QString& imdbId, const QString& /*title*/) {
                auto* mv = m_dependencies.movieDetail;
                if (!mv || imdbId.isEmpty()) {
                    return;
                }
                mv->clear();
                mv->load(imdbId);
                Q_EMIT showStreamsRequested(mv);
            });
    connect(libraryVm,
            &LibraryViewModel::openSeriesEpisodeStreamsRequested,
            this,
            [this](const QString& imdbId, const QString& title, int season, int episode) {
                auto* sv = m_dependencies.seriesDetail;
                if (!sv || imdbId.isEmpty() || season <= 0 || episode <= 0) {
                    // Fall back to the detail page if anything is
                    // missing -- safer than dropping the click.
                    openSeriesDetailAt(imdbId, title, season, episode);
                    return;
                }
                sv->clear();
                sv->loadAt(imdbId, season, episode);
                Q_EMIT showStreamsRequested(sv);
            });

    // Discover navigation routing. "Show all" forwards into the
    // Browse VM via a typed (kind, sort) preset and asks the shell
    // to navigate. Poster activation routes movies and series into
    // their respective detail VMs.
    connect(
        discoverVm, &DiscoverViewModel::browseRequested, this, &ShellViewModel::applyBrowsePreset);
    connect(discoverVm,
            &DiscoverViewModel::openMovieRequested,
            this,
            &ShellViewModel::openMovieDetailByTmdb);
    connect(discoverVm,
            &DiscoverViewModel::openSeriesRequested,
            this,
            &ShellViewModel::openSeriesDetailByTmdb);
    connect(discoverVm, &DiscoverViewModel::statusMessage, this, &ShellViewModel::passiveMessage);
    connect(discoverVm,
            &DiscoverViewModel::findMovieStreamsByTmdbRequested,
            this,
            &ShellViewModel::findMovieStreamsByTmdb);
    connect(discoverVm,
            &DiscoverViewModel::findSeriesStreamsByTmdbRequested,
            this,
            &ShellViewModel::findSeriesStreamsByTmdb);

    // Search VM action routing. Both movie and series activations
    // now push their respective detail page directly.
    connect(searchVm, &SearchViewModel::statusMessage, this, &ShellViewModel::passiveMessage);
    connect(searchVm, &SearchViewModel::openMovieRequested, this, &ShellViewModel::openMovieDetail);
    connect(
        searchVm, &SearchViewModel::openSeriesRequested, this, &ShellViewModel::openSeriesDetail);
    connect(searchVm,
            &SearchViewModel::findMovieStreamsByImdbRequested,
            this,
            &ShellViewModel::findMovieStreamsByImdb);
    connect(searchVm,
            &SearchViewModel::findSeriesStreamsByImdbRequested,
            this,
            &ShellViewModel::findSeriesStreamsByImdb);

    // Browse VM action routing.
    connect(browseVm, &BrowseViewModel::statusMessage, this, &ShellViewModel::passiveMessage);
    connect(browseVm,
            &BrowseViewModel::openMovieRequested,
            this,
            &ShellViewModel::openMovieDetailByTmdb);
    connect(browseVm,
            &BrowseViewModel::openSeriesRequested,
            this,
            &ShellViewModel::openSeriesDetailByTmdb);
    connect(browseVm,
            &BrowseViewModel::findMovieStreamsByTmdbRequested,
            this,
            &ShellViewModel::findMovieStreamsByTmdb);
    connect(browseVm,
            &BrowseViewModel::findSeriesStreamsByTmdbRequested,
            this,
            &ShellViewModel::findSeriesStreamsByTmdb);

    // Detail VM → shell.
    const auto pushSubtitlesFromDetail = [this](const domain::PlaybackContext& ctx) {
        pushSubtitlesPage(ctx, /*fromPlayer=*/false);
    };
    connect(
        movieDetailVm, &MovieDetailViewModel::statusMessage, this, &ShellViewModel::passiveMessage);
    connect(movieDetailVm,
            &MovieDetailViewModel::openMovieByTmdbRequested,
            this,
            &ShellViewModel::openMovieDetailByTmdb);
    connect(movieDetailVm,
            &MovieDetailViewModel::openSeriesByTmdbRequested,
            this,
            &ShellViewModel::openSeriesDetailByTmdb);
    connect(
        movieDetailVm, &MovieDetailViewModel::subtitlesRequested, this, pushSubtitlesFromDetail);
    connect(movieDetailVm, &MovieDetailViewModel::streamsRequested, this, [this] {
        Q_EMIT showStreamsRequested(m_dependencies.movieDetail);
    });
    connect(movieDetailVm,
            &MovieDetailViewModel::findMovieStreamsByTmdbRequested,
            this,
            &ShellViewModel::findMovieStreamsByTmdb);
    connect(movieDetailVm,
            &MovieDetailViewModel::findSeriesStreamsByTmdbRequested,
            this,
            &ShellViewModel::findSeriesStreamsByTmdb);

    connect(seriesDetailVm,
            &SeriesDetailViewModel::statusMessage,
            this,
            &ShellViewModel::passiveMessage);
    connect(seriesDetailVm,
            &SeriesDetailViewModel::openMovieByTmdbRequested,
            this,
            &ShellViewModel::openMovieDetailByTmdb);
    connect(seriesDetailVm,
            &SeriesDetailViewModel::openSeriesByTmdbRequested,
            this,
            &ShellViewModel::openSeriesDetailByTmdb);
    connect(
        seriesDetailVm, &SeriesDetailViewModel::subtitlesRequested, this, pushSubtitlesFromDetail);
    connect(seriesDetailVm, &SeriesDetailViewModel::streamsRequested, this, [this] {
        Q_EMIT showStreamsRequested(m_dependencies.seriesDetail);
    });
    connect(seriesDetailVm,
            &SeriesDetailViewModel::findMovieStreamsByTmdbRequested,
            this,
            &ShellViewModel::findMovieStreamsByTmdb);
    connect(seriesDetailVm,
            &SeriesDetailViewModel::findSeriesStreamsByTmdbRequested,
            this,
            &ShellViewModel::findSeriesStreamsByTmdb);

    // Subtitles VM. Routes download / local-file / settings
    // requests back through this shell.
    connect(subtitlesVm, &SubtitlesViewModel::settingsRequested, this, [this] {
        requestSettings(QStringLiteral("subtitles"));
    });
    connect(subtitlesVm, &SubtitlesViewModel::closeRequested, this, [this] {
        Q_EMIT popPageRequested();
    });

#ifdef KINEMA_HAVE_LIBMPV
    auto* mpris = m_dependencies.mpris;
    auto* playerLauncher = m_dependencies.player;
    if (mpris) {
        connect(mpris, &playback::desktop::MprisPlaybackProjection::raiseRequested, this, [this] {
            if (m_playerWindow && m_playerWindow->hasEverLoaded()) {
                m_playerWindow->show();
                m_playerWindow->raise();
                m_playerWindow->requestActivate();
                return;
            }
            if (m_window) {
                m_window->setVisible(true);
                m_window->raise();
                m_window->requestActivate();
            }
        });
        connect(mpris,
                &playback::desktop::MprisPlaybackProjection::quitRequested,
                this,
                &ShellViewModel::requestQuit);
    }

    connect(playerLauncher,
            &core::PlayerLauncher::embeddedRequested,
            this,
            &ShellViewModel::openEmbeddedPlayer);
#endif
}

} // namespace kinema::ui::qml
