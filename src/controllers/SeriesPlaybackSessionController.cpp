// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "controllers/SeriesPlaybackSessionController.h"

#include "controllers/PlaybackController.h"
#include "download/DownloadManager.h"
#include "services/StreamActions.h"
#include "torrent/TorrentStreamingService.h"

#include "kinema_log_player.h"

#include <QFileInfo>

namespace kinema::controllers {

SeriesPlaybackSessionController::SeriesPlaybackSessionController(
    PlaybackController& playback,
    torrent::TorrentStreamingService& torrentStreaming,
    services::StreamActions& actions,
    download::DownloadManager* downloadManager,
    QObject* parent)
    : QObject(parent)
    , m_playback(playback)
    , m_torrentStreaming(torrentStreaming)
    , m_actions(actions)
    , m_downloadManager(downloadManager)
{
}

bool SeriesPlaybackSessionController::navigationVisible() const noexcept
{
    return m_navigationVisible;
}

bool SeriesPlaybackSessionController::canGoPrevious() const noexcept
{
    return m_navigationVisible && m_previous.has_value();
}

bool SeriesPlaybackSessionController::canGoNext() const noexcept
{
    return m_navigationVisible && m_next.has_value();
}

void SeriesPlaybackSessionController::refreshFromPlayback(bool active)
{
    m_userClosed = false;
    if (!active) {
        return;
    }

    const auto& ctx = m_playback.currentContext();
    if (ctx.key.kind != domain::MediaKind::Series
        || !ctx.key.season.has_value()
        || !ctx.key.episode.has_value()
        || ctx.streamRef.infoHash.isEmpty()) {
        qCDebug(KINEMA_PLAYER) << "series-pack: skipping refresh"
            << "kind=" << static_cast<int>(ctx.key.kind)
            << "hasSeason=" << ctx.key.season.has_value()
            << "hasEpisode=" << ctx.key.episode.has_value()
            << "infoHashEmpty=" << ctx.streamRef.infoHash.isEmpty();
        clearState();
        return;
    }

    // Prefer the unified download-manager lookup. This covers both
    // libtorrent sessions and HTTP-backed debrid sessions, the
    // latter of which never appear in `TorrentStreamingService` at
    // all. Fall back to the streaming service only when the
    // manager has no session for the hash (e.g. the legacy
    // `StreamActions::playTorrentTask` path or test setups that
    // wire the controller without a download manager).
    QVector<torrent::TorrentFileEntry> files;
    const char* fileSource = "none";
    if (m_downloadManager) {
        files = m_downloadManager->filesForInfoHash(
            ctx.streamRef.infoHash);
        if (!files.isEmpty()) {
            fileSource = "download-manager";
        }
    }
    if (files.isEmpty()) {
        files = m_torrentStreaming.filesForInfoHash(
            ctx.streamRef.infoHash);
        if (!files.isEmpty()) {
            fileSource = "torrent-streaming";
        }
    }
    const int playable = torrent::playableCandidateCount(files);
    qCDebug(KINEMA_PLAYER).nospace()
        << "series-pack: filesForInfoHash source=" << fileSource
        << " files=" << files.size() << " playable=" << playable
        << " target=S" << *ctx.key.season << "E" << *ctx.key.episode
        << " pinnedFileIndex=" << ctx.streamRef.fileIndex;
    if (files.isEmpty()) {
        qCDebug(KINEMA_PLAYER)
            << "series-pack: clearing — empty file list (metadata"
            << "likely not yet resolved)";
        clearState();
        return;
    }

    const auto nav = torrent::adjacentEpisodeFiles(files,
        *ctx.key.season, *ctx.key.episode);
    if (!nav || !nav->current.has_value()) {
        qCDebug(KINEMA_PLAYER)
            << "series-pack: clearing — no current-episode match"
            << "(parse miss / ambiguous duplicate / single-episode"
            << "torrent)";
        clearState();
        return;
    }

    qCDebug(KINEMA_PLAYER).nospace()
        << "series-pack: adjacency previous="
        << (nav->previous ? QStringLiteral("S%1E%2")
                .arg(nav->previous->season).arg(nav->previous->episode)
                          : QStringLiteral("—"))
        << " current=S" << nav->current->season << "E"
        << nav->current->episode
        << " next="
        << (nav->next ? QStringLiteral("S%1E%2")
                .arg(nav->next->season).arg(nav->next->episode)
                      : QStringLiteral("—"));

    if (ctx.streamRef.fileIndex >= 0
        && nav->current->file.index != ctx.streamRef.fileIndex) {
        qCDebug(KINEMA_PLAYER).nospace()
            << "series-pack: clearing — fileIndex mismatch (pinned="
            << ctx.streamRef.fileIndex << ", parsed="
            << nav->current->file.index << ")";
        clearState();
        return;
    }

    auto toTarget = [this](const torrent::EpisodeFileTarget& target)
        -> EpisodeTarget {
        domain::PlaybackKey key;
        key.kind = domain::MediaKind::Series;
        key.imdbId = m_playback.currentContext().key.imdbId;
        key.season = target.season;
        key.episode = target.episode;
        return {
            key,
            target.file.index,
            QFileInfo(target.file.path).fileName(),
            target.file.size,
        };
    };

    setState(ctx,
        nav->previous ? std::make_optional(toTarget(*nav->previous))
                      : std::nullopt,
        nav->next ? std::make_optional(toTarget(*nav->next))
                  : std::nullopt);

    // Hydrate the currently playing file's size onto the picker
    // row when the debrid provider (or libtorrent metadata) gave
    // us a definitive byte count and the original Torrentio parse
    // didn't surface one. De-dup by (infoHash, fileIndex, size) so
    // resume / seek refreshes don't re-fire the signal.
    if (nav->current->file.size > 0) {
        const QString sizeKey = ctx.streamRef.infoHash
            + QStringLiteral(":%1:%2")
                  .arg(nav->current->file.index)
                  .arg(nav->current->file.size);
        if (sizeKey != m_lastSizeHydrationKey) {
            m_lastSizeHydrationKey = sizeKey;
            Q_EMIT currentStreamSizeResolved(ctx.streamRef.infoHash,
                nav->current->file.index, nav->current->file.size);
        }
    }

    // Surface the adjacency outcome to the UI so the picker badge
    // can be paired with a one-shot status confirmation. The
    // "is this a pack?" question is gated on `playable > 1` because
    // a single-file torrent landing here is just a normal episode
    // playback and shouldn't trigger picker feedback either way.
    const QString adjacencyKey = ctx.streamRef.infoHash
        + QStringLiteral(":S%1E%2")
            .arg(*ctx.key.season).arg(*ctx.key.episode);
    if (playable > 1 && adjacencyKey != m_lastAdjacencyKey) {
        m_lastAdjacencyKey = adjacencyKey;
        if (m_next.has_value()) {
            Q_EMIT packAdjacencyResolved(true,
                m_next->key.season.value_or(0),
                m_next->key.episode.value_or(0));
        } else {
            Q_EMIT packAdjacencyResolved(false, 0, 0);
        }
    }
}

void SeriesPlaybackSessionController::onPlayerEndOfFile(
    const QString& reason,
    const domain::PlaybackContext& ctx)
{
    // Contract: `PlaybackController::endOfFile(reason, ctx)`
    // delivers the ctx of the file that *actually* ended
    // (`m_loadedCtx`-scoped over there). The loadfile-induced
    // intermediate stop is absorbed inside PlaybackController and
    // never reaches us, so during the normal flow `ctx.key` and
    // `m_baseContext.key` always agree.
    //
    // This guard is belt-and-braces: if a future regression
    // upstream ever lets a stale end-file leak through, fail safe
    // by ignoring it rather than clobbering the freshly-set prev/
    // next state and blanking the transport chrome.
    const bool isCurrent = m_baseContext.key.isValid()
        && m_baseContext.key == ctx.key;
    if (!isCurrent) {
        qCDebug(KINEMA_PLAYER).nospace()
            << "series-pack: ignoring end-file reason=\"" << reason
            << "\" for superseded ctx (current="
            << m_baseContext.key.imdbId
            << " S" << m_baseContext.key.season.value_or(0)
            << "E" << m_baseContext.key.episode.value_or(0)
            << ", ended=" << ctx.key.imdbId
            << " S" << ctx.key.season.value_or(0)
            << "E" << ctx.key.episode.value_or(0) << ")";
        return;
    }

    if (m_userClosed || reason == QStringLiteral("stop")) {
        m_userClosed = false;
        clearState();
        return;
    }

    const bool cleanEof = reason.isEmpty() || reason == QStringLiteral("eof");
    if (cleanEof && m_next.has_value()) {
        playTarget(*m_next);
        return;
    }

    clearState();
    Q_EMIT windowCloseRequested();
}

void SeriesPlaybackSessionController::onPlayerUserClosed(
    const domain::PlaybackContext& ctx)
{
    Q_UNUSED(ctx);
    m_userClosed = true;
    clearState();
}

void SeriesPlaybackSessionController::playPreviousEpisode()
{
    if (m_previous.has_value()) {
        playTarget(*m_previous);
    }
}

void SeriesPlaybackSessionController::playNextEpisode()
{
    if (m_next.has_value()) {
        playTarget(*m_next);
    }
}

void SeriesPlaybackSessionController::clearState()
{
    const bool changed = m_navigationVisible
        || m_previous.has_value()
        || m_next.has_value()
        || !m_baseContext.key.imdbId.isEmpty();
    m_navigationVisible = false;
    m_previous.reset();
    m_next.reset();
    m_baseContext = {};
    m_lastAdjacencyKey.clear();
    m_lastSizeHydrationKey.clear();
    if (changed) {
        Q_EMIT navigationChanged();
    }
}

void SeriesPlaybackSessionController::setState(
    const domain::PlaybackContext& ctx,
    std::optional<EpisodeTarget> previous,
    std::optional<EpisodeTarget> next)
{
    const bool visible = previous.has_value() || next.has_value();
    const bool changed = m_navigationVisible != visible
        || m_previous.has_value() != previous.has_value()
        || m_next.has_value() != next.has_value()
        || m_baseContext.key != ctx.key;

    m_baseContext = ctx;
    m_previous = std::move(previous);
    m_next = std::move(next);
    m_navigationVisible = visible;

    if (changed) {
        Q_EMIT navigationChanged();
    }
}

void SeriesPlaybackSessionController::playTarget(const EpisodeTarget& target)
{
    if (!target.key.isValid() || m_baseContext.streamRef.infoHash.isEmpty()) {
        return;
    }

    domain::Stream stream;
    stream.infoHash = m_baseContext.streamRef.infoHash;
    stream.releaseName = m_baseContext.streamRef.releaseName;
    stream.resolution = m_baseContext.streamRef.resolution;
    stream.qualityLabel = m_baseContext.streamRef.qualityLabel;
    stream.provider = m_baseContext.streamRef.provider;
    stream.fileIndex = target.fileIndex;
    stream.fileNameHint = target.fileNameHint;
    if (target.sizeBytes > 0) {
        stream.sizeBytes = target.sizeBytes;
    }

    domain::PlaybackContext ctx;
    ctx.key = target.key;
    ctx.seriesTitle = m_baseContext.seriesTitle;
    ctx.episodeTitle = episodeCode(target.key.season.value_or(0),
        target.key.episode.value_or(0));
    ctx.title = displayTitle(m_baseContext,
        target.key.season.value_or(0),
        target.key.episode.value_or(0));
    ctx.poster = m_baseContext.poster;

    m_actions.play(stream, ctx);
}

QString SeriesPlaybackSessionController::episodeCode(int season, int episode)
{
    return QStringLiteral("S%1E%2")
        .arg(season, 2, 10, QLatin1Char('0'))
        .arg(episode, 2, 10, QLatin1Char('0'));
}

QString SeriesPlaybackSessionController::displayTitle(
    const domain::PlaybackContext& base,
    int season,
    int episode)
{
    const auto code = episodeCode(season, episode);
    if (base.seriesTitle.isEmpty()) {
        return code;
    }
    return QStringLiteral("%1 — %2")
        .arg(base.seriesTitle, code);
}

} // namespace kinema::controllers

#endif // KINEMA_HAVE_LIBMPV
