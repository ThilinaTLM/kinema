// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/progress/PlaybackProgressProjector.h"

#include "playback/events/PlaybackEventStream.h"
#include "playback/ports/PlaybackHistoryRepository.h"

#include <QDateTime>

#include <cmath>

namespace kinema::playback::progress {

namespace {

QString selectedAudioLang(const core::tracks::TrackList& tracks)
{
    for (const auto& track : tracks) {
        if (track.type == QLatin1String("audio") && track.selected) {
            return track.lang;
        }
    }
    return {};
}

QString selectedSubtitleLang(const core::tracks::TrackList& tracks)
{
    bool sawSubtitle = false;
    for (const auto& track : tracks) {
        if (track.type != QLatin1String("sub")) {
            continue;
        }
        sawSubtitle = true;
        if (track.selected) {
            return track.lang;
        }
    }
    return sawSubtitle ? QStringLiteral("off") : QString {};
}

template<class... Ts>
struct overloaded : Ts... {
    using Ts::operator()...;
};
template<class... Ts>
overloaded(Ts...) -> overloaded<Ts...>;

} // namespace

PlaybackProgressProjector::PlaybackProgressProjector(
    ports::PlaybackHistoryRepository& repo,
    events::PlaybackEventStream& stream,
    QObject* parent)
    : QObject(parent)
    , m_repo(repo)
{
    connect(&stream, &events::PlaybackEventStream::eventPublished,
        this, &PlaybackProgressProjector::onEvent);
}

PlaybackProgressProjector::~PlaybackProgressProjector() = default;

void PlaybackProgressProjector::setPersistIntervalSeconds(double s) noexcept
{
    m_persistIntervalSec = s;
}

void PlaybackProgressProjector::setMinProgressFraction(double f) noexcept
{
    m_minProgressFraction = f;
}

void PlaybackProgressProjector::onEvent(const events::PlaybackEvent& event)
{
    std::visit(
        overloaded {
            [this](const events::PlaybackRequested& e) { onPlaybackRequested(e); },
            [this](const events::PlayerLoaded& e) { onPlayerLoaded(e); },
            [this](const events::PositionTicked& e) { onPositionTicked(e); },
            [this](const events::DurationChanged& e) { onDurationChanged(e); },
            [this](const events::TrackListChanged& e) { onTrackListChanged(e); },
            [this](const events::ChapterListChanged& e) { onChapterListChanged(e); },
            [this](const events::PlaybackEnded& e) { onPlaybackEnded(e); },
            [this](const events::PlaybackFailed& e) { onPlaybackFailed(e); },
            [](const auto&) { /* ignore */ },
        },
        event);
}

void PlaybackProgressProjector::onPlaybackRequested(
    const events::PlaybackRequested& e)
{
    if (!e.ctx.key.isValid()) {
        return;
    }

    // If a different session is active, persist its in-memory
    // position before we lose it.
    if (m_active && !(m_active->key == e.ctx.key)) {
        persistActive(/*force=*/true);
    }

    m_active = e.ctx;
    m_activeSessionId = e.sessionId;
    m_lastPosition = 0.0;
    m_duration = 0.0;
    m_lastPersistedPosition = 0.0;
    m_activeChapters.clear();
    m_rememberedAudioLang.clear();
    m_rememberedSubtitleLang.clear();

    // Seed / refresh the history row immediately so external plays
    // appear in Continue Watching from the moment the launcher is
    // fired, and the stored stream reference is up-to-date even
    // before the first position tick arrives.
    domain::HistoryEntry entry;
    entry.key = e.ctx.key;
    entry.title = e.ctx.title;
    entry.seriesTitle = e.ctx.seriesTitle;
    entry.episodeTitle = e.ctx.episodeTitle;
    entry.poster = e.ctx.poster;
    entry.backdrop = e.ctx.backdrop;
    entry.lastStream = e.ctx.streamRef;
    entry.lastWatchedAt = QDateTime::currentDateTimeUtc();

    // Preserve any existing progress on an upsert; treat a
    // re-watched finished row as a fresh attempt so it reappears
    // in Continue Watching.
    if (const auto existing = m_repo.find(e.ctx.key)) {
        entry.positionSec = existing->positionSec;
        entry.durationSec = existing->durationSec;
        entry.finished = false;
    }
    m_repo.record(entry);
}

void PlaybackProgressProjector::onPlayerLoaded(const events::PlayerLoaded& e)
{
    if (!m_active || e.sessionId != m_activeSessionId) {
        return;
    }
    m_lastPosition = 0.0;
    m_lastPersistedPosition = 0.0;
    // Fresh file → stale chapter list from a previous session must
    // go; the adapter will re-emit ChapterListChanged when mpv
    // discovers the new file's chapters.
    m_activeChapters.clear();
}

void PlaybackProgressProjector::onPositionTicked(
    const events::PositionTicked& e)
{
    if (!m_active || e.sessionId != m_activeSessionId) {
        return;
    }
    if (e.seconds < 0.0) {
        return;
    }
    m_lastPosition = e.seconds;

    if (m_duration <= 0.0) {
        return;
    }
    if (e.seconds / m_duration < m_minProgressFraction) {
        return;
    }
    if (std::abs(e.seconds - m_lastPersistedPosition) < m_persistIntervalSec) {
        return;
    }
    persistActive(/*force=*/false);
}

void PlaybackProgressProjector::onDurationChanged(
    const events::DurationChanged& e)
{
    if (!m_active || e.sessionId != m_activeSessionId) {
        return;
    }
    if (e.seconds > 0.0) {
        m_duration = e.seconds;
    }
}

void PlaybackProgressProjector::onTrackListChanged(
    const events::TrackListChanged& e)
{
    if (!m_active || e.sessionId != m_activeSessionId) {
        return;
    }
    m_rememberedAudioLang = selectedAudioLang(e.tracks);
    m_rememberedSubtitleLang = selectedSubtitleLang(e.tracks);
}

void PlaybackProgressProjector::onChapterListChanged(
    const events::ChapterListChanged& e)
{
    if (!m_active || e.sessionId != m_activeSessionId) {
        return;
    }
    m_activeChapters = e.chapters;
}

void PlaybackProgressProjector::onPlaybackEnded(const events::PlaybackEnded& e)
{
    if (!m_active || e.sessionId != m_activeSessionId) {
        return;
    }
    // ReplacedByNewSource is a session bookkeeping signal: the user
    // didn't stop; another play is taking over. Persist the live
    // position but do NOT apply the session-end policy (which
    // could mark watched depending on threshold). The new
    // PlaybackRequested already starting will fold the new context
    // in.
    if (e.reason == PlaybackEndReason::ReplacedByNewSource) {
        persistActive(/*force=*/true);
        m_active.reset();
        m_activeSessionId = PlaybackSessionId();
        m_lastPosition = 0.0;
        m_duration = 0.0;
        m_lastPersistedPosition = 0.0;
        m_activeChapters.clear();
        return;
    }
    const auto entry = buildActiveEntry();
    const auto creditsStart = core::chapters::findCreditsStart(
        m_activeChapters, m_duration);
    std::optional<double> creditsOpt;
    if (creditsStart.has_value()) {
        creditsOpt = *creditsStart;
    }
    m_repo.recordSessionEnd(entry, e.reason, creditsOpt);
    m_lastPersistedPosition = m_lastPosition;
    m_active.reset();
    m_activeSessionId = PlaybackSessionId();
    m_lastPosition = 0.0;
    m_duration = 0.0;
    m_lastPersistedPosition = 0.0;
    m_activeChapters.clear();
}

void PlaybackProgressProjector::onPlaybackFailed(const events::PlaybackFailed& e)
{
    if (!m_active || e.sessionId != m_activeSessionId) {
        return;
    }
    // PlayerError: persist the final position without flipping
    // finished — mirrors HistoryStore's Error branch.
    const auto entry = buildActiveEntry();
    m_repo.recordSessionEnd(entry, PlaybackEndReason::PlayerError,
        std::nullopt);
    m_active.reset();
    m_activeSessionId = PlaybackSessionId();
    m_lastPosition = 0.0;
    m_duration = 0.0;
    m_lastPersistedPosition = 0.0;
    m_activeChapters.clear();
}

void PlaybackProgressProjector::persistActive(bool force)
{
    if (!m_active) {
        return;
    }
    if (!force && std::abs(m_lastPosition - m_lastPersistedPosition)
            < m_persistIntervalSec) {
        return;
    }
    m_repo.record(buildActiveEntry());
    m_lastPersistedPosition = m_lastPosition;
}

domain::HistoryEntry PlaybackProgressProjector::buildActiveEntry() const
{
    domain::HistoryEntry entry;
    if (!m_active) {
        return entry;
    }
    entry.key = m_active->key;
    entry.title = m_active->title;
    entry.seriesTitle = m_active->seriesTitle;
    entry.episodeTitle = m_active->episodeTitle;
    entry.poster = m_active->poster;
    entry.backdrop = m_active->backdrop;
    entry.lastStream = m_active->streamRef;
    entry.positionSec = m_lastPosition;
    entry.durationSec = m_duration;
    entry.lastWatchedAt = QDateTime::currentDateTimeUtc();
    entry.rememberedAudioLang = m_rememberedAudioLang;
    entry.rememberedSubtitleLang = m_rememberedSubtitleLang;
    return entry;
}

} // namespace kinema::playback::progress
