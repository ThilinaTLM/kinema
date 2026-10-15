// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/series/SeriesSessionService.h"

#include "domain/Media.h"
#include "kinema_log_player.h"
#include "playback/events/PlaybackEventStream.h"
#include "playback/policy/MediaFileSelectionPolicy.h"
#include "playback/ports/SessionFileCatalog.h"
#include "playback/session/PlaybackSessionManager.h"

#include <QFileInfo>

namespace kinema::playback::series {

SeriesSessionService::SeriesSessionService(
    events::PlaybackEventStream& eventsBus,
    ports::SessionFileCatalog& catalog,
    session::PlaybackSessionManager& sessions, QObject* parent)
    : QObject(parent)
    , m_events(eventsBus)
    , m_catalog(catalog)
    , m_sessions(sessions)
{
    // Direct-connect: every playback event interesting to us is
    // already on the GUI thread, and we want adjacency to land
    // synchronously with the originating event.
    connect(&m_events, &events::PlaybackEventStream::eventPublished,
        this, &SeriesSessionService::onEvent);
}

SeriesSessionService::~SeriesSessionService() = default;

void SeriesSessionService::onEvent(const events::PlaybackEvent& e)
{
    std::visit(
        [this](const auto& payload) {
            using T = std::decay_t<decltype(payload)>;
            if constexpr (std::is_same_v<T, events::PlaybackRequested>) {
                onPlaybackRequested(payload);
            } else if constexpr (std::is_same_v<T, events::PlayerLoaded>) {
                onPlayerLoaded(payload);
            } else if constexpr (std::is_same_v<T, events::PlaybackEnded>) {
                onPlaybackEnded(payload);
            } else if constexpr (std::is_same_v<T, events::PlaybackFailed>) {
                onPlaybackFailed(payload);
            }
        },
        e);
}

void SeriesSessionService::onPlaybackRequested(
    const events::PlaybackRequested& e)
{
    // A fresh attempt supersedes any prior tracking. Wipe the
    // adjacency state so the navigation chrome doesn't briefly
    // show prev/next from the previous episode while the new
    // session's catalog lookup runs.
    m_currentSessionId = e.sessionId;
    m_baseContext = e.ctx;
    m_lastAdjacencyKey.clear();
    m_lastSizeHydrationKey.clear();
    if (m_navigationVisible || m_previous.has_value()
        || m_next.has_value()) {
        m_navigationVisible = false;
        m_previous.reset();
        m_next.reset();
        Q_EMIT navigationChanged();
    }
}

void SeriesSessionService::onPlayerLoaded(
    const events::PlayerLoaded& e)
{
    if (e.sessionId != m_currentSessionId) {
        return;
    }
    resolveAdjacencyFromCatalog();
}

void SeriesSessionService::resolveAdjacencyFromCatalog()
{
    const auto& ctx = m_baseContext;
    if (ctx.key.kind != domain::MediaKind::Series
        || !ctx.key.season.has_value()
        || !ctx.key.episode.has_value()
        || ctx.streamRef.infoHash.isEmpty()) {
        qCDebug(KINEMA_PLAYER) << "series-pack: skipping resolve"
            << "kind=" << static_cast<int>(ctx.key.kind)
            << "hasSeason=" << ctx.key.season.has_value()
            << "hasEpisode=" << ctx.key.episode.has_value()
            << "infoHashEmpty=" << ctx.streamRef.infoHash.isEmpty();
        clearState();
        return;
    }

    const auto files = m_catalog.filesForStreamRef(ctx.streamRef);
    const int playable = policy::playableCandidateCount(files);
    qCDebug(KINEMA_PLAYER).nospace()
        << "series-pack: resolveAdjacency files=" << files.size()
        << " playable=" << playable
        << " target=S" << *ctx.key.season << "E" << *ctx.key.episode
        << " pinnedFileIndex=" << ctx.streamRef.fileIndex;
    if (files.isEmpty()) {
        qCDebug(KINEMA_PLAYER)
            << "series-pack: clearing — empty file list (metadata"
            << "likely not yet resolved)";
        clearState();
        return;
    }

    const auto nav = policy::adjacentEpisodeFiles(files,
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

    // The pinned `fileIndex` originates from Torrentio's `fileIdx`,
    // which references the torrent's libtorrent-metadata file
    // order. `nav->current->file.index` originates from whichever
    // catalog returned the file list: libtorrent for torrent-backed
    // sessions (indices align) or the resolver's flattened list
    // for HTTP-backed sessions (Real-Debrid / AllDebrid; indices
    // are positional in the provider's directory walk and routinely
    // differ from the torrent's order). Because
    // `adjacentEpisodeFiles` only surfaces a `current` when exactly
    // one playable file in the pack parses to the requested
    // (season, episode), the parser-side identity is already
    // sufficient. Keep the diagnostic for support reports but do
    // not blank the navigation chrome.
    if (ctx.streamRef.fileIndex >= 0
        && nav->current->file.index != ctx.streamRef.fileIndex) {
        qCDebug(KINEMA_PLAYER).nospace()
            << "series-pack: fileIndex drift (pinned="
            << ctx.streamRef.fileIndex << ", parsed="
            << nav->current->file.index
            << ") — accepting parser identity; indices come from"
               " different sources for debrid-backed sessions";
    }

    auto toTarget
        = [this](const domain::EpisodeFileTarget& target) -> EpisodeTarget {
        domain::PlaybackKey key;
        key.kind = domain::MediaKind::Series;
        key.imdbId = m_baseContext.key.imdbId;
        key.season = target.season;
        key.episode = target.episode;
        return {
            key,
            target.file.index,
            QFileInfo(target.file.path).fileName(),
            target.file.size,
        };
    };

    setState(nav->previous ? std::make_optional(toTarget(*nav->previous))
                           : std::nullopt,
        nav->next ? std::make_optional(toTarget(*nav->next))
                  : std::nullopt);

    // Hydrate the currently playing file's size onto the picker
    // row when the catalog gave us a definitive byte count and the
    // original Torrentio parse didn't surface one. De-dup by
    // (infoHash, fileIndex, size) so repeated PlayerLoaded events
    // (e.g. from resume / seek transitions) don't re-fire the
    // signal.
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
    // can be paired with a one-shot status confirmation. Gated on
    // `playable > 1` because a single-file torrent landing here is
    // just a normal episode playback and shouldn't trigger picker
    // feedback either way.
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

void SeriesSessionService::onPlaybackEnded(
    const events::PlaybackEnded& e)
{
    if (e.sessionId != m_currentSessionId) {
        // Stale end from a superseded attempt. The legacy controller
        // had the same guard; matching by session id makes it strict
        // by construction.
        qCDebug(KINEMA_PLAYER).nospace()
            << "series-pack: ignoring end for superseded session";
        return;
    }

    using R = PlaybackEndReason;
    switch (e.reason) {
    case R::UserStop:
    case R::PlayerError:
    case R::LoadTimeout:
        clearState();
        return;
    case R::ReplacedByNewSource:
        // Will be followed by a fresh PlaybackRequested for the
        // replacement; let that drive state.
        return;
    case R::NaturalEof:
        break;
    }

    // Clean EOF: dispatch next if available, otherwise ask the
    // shell to close the window.
    if (m_next.has_value()) {
        const auto target = *m_next;
        clearState();
        dispatch(target);
        return;
    }
    clearState();
    Q_EMIT windowCloseRequested();
}

void SeriesSessionService::onPlaybackFailed(
    const events::PlaybackFailed& e)
{
    if (e.sessionId != m_currentSessionId) {
        return;
    }
    clearState();
}

void SeriesSessionService::playPreviousEpisode()
{
    if (m_previous.has_value()) {
        const auto target = *m_previous;
        dispatch(target);
    }
}

void SeriesSessionService::playNextEpisode()
{
    if (m_next.has_value()) {
        const auto target = *m_next;
        dispatch(target);
    }
}

void SeriesSessionService::clearState()
{
    const bool changed = m_navigationVisible || m_previous.has_value()
        || m_next.has_value();
    m_navigationVisible = false;
    m_previous.reset();
    m_next.reset();
    if (changed) {
        Q_EMIT navigationChanged();
    }
}

void SeriesSessionService::setState(
    std::optional<EpisodeTarget> previous,
    std::optional<EpisodeTarget> next)
{
    const bool visible = previous.has_value() || next.has_value();
    const bool changed = m_navigationVisible != visible
        || m_previous.has_value() != previous.has_value()
        || m_next.has_value() != next.has_value();
    m_previous = std::move(previous);
    m_next = std::move(next);
    m_navigationVisible = visible;
    if (changed) {
        Q_EMIT navigationChanged();
    }
}

void SeriesSessionService::dispatch(const EpisodeTarget& target)
{
    if (!target.key.isValid()
        || m_baseContext.streamRef.infoHash.isEmpty()) {
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

    m_sessions.play(stream, ctx);
}

QString SeriesSessionService::episodeCode(int season, int episode)
{
    return QStringLiteral("S%1E%2")
        .arg(season, 2, 10, QLatin1Char('0'))
        .arg(episode, 2, 10, QLatin1Char('0'));
}

QString SeriesSessionService::displayTitle(
    const domain::PlaybackContext& base, int season, int episode)
{
    const auto code = episodeCode(season, episode);
    if (base.seriesTitle.isEmpty()) {
        return code;
    }
    return QStringLiteral("%1 — %2").arg(base.seriesTitle, code);
}

} // namespace kinema::playback::series
