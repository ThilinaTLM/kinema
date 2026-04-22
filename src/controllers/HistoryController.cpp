// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/HistoryController.h"

#include "api/TorrentioClient.h"
#include "config/AppSettings.h"
#include "core/HistoryStore.h"
#include "core/HttpError.h"
#include "core/HttpErrorPresenter.h"
#include "kinema_debug.h"
#include "services/StreamActions.h"

#ifdef KINEMA_HAVE_LIBMPV
#include "ui/player/MpvWidget.h"
#include "ui/player/PlayerWindow.h"
#endif

#include <KLocalizedString>

#include <QDateTime>

namespace kinema::controllers {

namespace {

/// How often (in seconds of playback) we push the in-memory position
/// into the store. With WAL + synchronous=NORMAL a single UPDATE is
/// sub-millisecond, but there's no reason to issue ~4 writes/sec when
/// the stored value only needs 5-second granularity.
constexpr double kPersistIntervalSec = 5.0;

/// Minimum progress (fraction of duration) before we start persisting
/// ticks. Avoids recording "I opened a stream for 2 seconds" as
/// resume-able progress.
constexpr double kMinProgressFraction = 0.005;

#ifdef KINEMA_HAVE_LIBMPV
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
#endif

} // namespace

HistoryController::HistoryController(core::HistoryStore& store,
    api::TorrentioClient* torrentio,
    const config::AppSettings& settings,
    const QString& rdTokenRef,
    QObject* parent)
    : QObject(parent)
    , m_store(store)
    , m_torrentio(torrentio)
    , m_settings(settings)
    , m_rdToken(rdTokenRef)
{
    connect(&m_store, &core::HistoryStore::changed,
        this, &HistoryController::changed);
}

HistoryController::~HistoryController() = default;

void HistoryController::setPlayerWindow(ui::player::PlayerWindow* window)
{
#ifdef KINEMA_HAVE_LIBMPV
    if (m_player == window) {
        return;
    }
    if (m_player) {
        disconnect(m_player, nullptr, this, nullptr);
    }
    m_player = window;
    if (!m_player) {
        return;
    }
    connect(m_player, &ui::player::PlayerWindow::fileLoaded,
        this, &HistoryController::onFileLoaded);
    connect(m_player, &ui::player::PlayerWindow::endOfFile,
        this, &HistoryController::onEndOfFile);
    connect(m_player, &ui::player::PlayerWindow::positionChanged,
        this, &HistoryController::onPositionChanged);
    connect(m_player, &ui::player::PlayerWindow::durationChanged,
        this, &HistoryController::onDurationChanged);
#else
    Q_UNUSED(window);
#endif
}

void HistoryController::setStreamActions(services::StreamActions* actions)
{
    m_actions = actions;
}

void HistoryController::onPlayStarting(const api::PlaybackContext& ctx)
{
    if (!ctx.key.isValid()) {
        return;
    }

    // If there's an active embedded session for a different key,
    // commit its last position to disk before we lose it.
    if (m_active && !(m_active->key == ctx.key)) {
        persistActive(/*force=*/true);
        m_active.reset();
        m_lastPosition = 0.0;
        m_duration = 0.0;
        m_lastPersistedPosition = 0.0;
    }

    m_pending = ctx;

    // Seed / refresh the history row immediately so:
    //   - External plays appear in Continue Watching from the moment
    //     the launcher is fired.
    //   - The stored stream reference is up-to-date even before the
    //     first position tick arrives.
    api::HistoryEntry e;
    e.key = ctx.key;
    e.title = ctx.title;
    e.seriesTitle = ctx.seriesTitle;
    e.episodeTitle = ctx.episodeTitle;
    e.poster = ctx.poster;
    e.lastStream = ctx.streamRef;
    e.lastWatchedAt = QDateTime::currentDateTimeUtc();

    // Preserve any existing progress on an upsert. The store merges
    // position_sec from `excluded`, so we read-back here to avoid
    // overwriting a saved position with 0 on a fresh play.
    if (const auto existing = m_store.find(ctx.key)) {
        e.positionSec = existing->positionSec;
        e.durationSec = existing->durationSec;
        // If the user re-opens an already-finished movie, treat that
        // as a fresh watch (clear finished) so it reappears in
        // Continue Watching.
        e.finished = false;
    }

    m_store.record(e);
}

std::optional<qint64> HistoryController::resumeSecondsFor(
    const api::PlaybackKey& key) const
{
    if (!key.isValid()) {
        return std::nullopt;
    }

    // Prefer the absolutely-fresh in-memory position of the currently
    // playing session. This is the mid-session stream-swap case:
    // position ticks are still flowing, disk is 5 s stale at most,
    // memory is current.
    if (m_active && m_active->key == key
        && m_lastPosition > 1.0) {
        return static_cast<qint64>(m_lastPosition);
    }

    const auto stored = m_store.find(key);
    if (!stored) {
        return std::nullopt;
    }
    if (stored->finished) {
        return std::nullopt;
    }
    if (stored->positionSec < 1.0) {
        return std::nullopt;
    }
    // Clamp a bit below duration so we don't resume onto the end
    // credits; the store's threshold should already have caught
    // "effectively done" but defence in depth.
    double resume = stored->positionSec;
    if (stored->durationSec > 10.0) {
        resume = qMin(resume, stored->durationSec - 5.0);
    }
    return static_cast<qint64>(qMax(0.0, resume));
}

std::optional<api::HistoryEntry> HistoryController::find(
    const api::PlaybackKey& key) const
{
    return m_store.find(key);
}

std::optional<api::HistoryEntry> HistoryController::findLatestForMedia(
    api::MediaKind kind, const QString& imdbId) const
{
    return m_store.findLatestForMedia(kind, imdbId);
}

QList<api::HistoryEntry> HistoryController::continueWatching(
    int maxItems) const
{
    return m_store.continueWatching(maxItems);
}

void HistoryController::resumeFromHistory(const api::HistoryEntry& entry)
{
    if (!entry.key.isValid()) {
        return;
    }
    // We need at least one of (infoHash, releaseName) to re-resolve
    // the saved release against Torrentio. Missing both means the
    // entry predates the stream-tracking code or was recorded
    // without a chosen release — no way to find it again.
    if (entry.lastStream.infoHash.isEmpty()
        && entry.lastStream.releaseName.isEmpty()) {
        qCInfo(KINEMA)
            << "HistoryController: cannot resume" << entry.key.storageKey()
            << "— no saved release reference (delete + replay to recover)";
        Q_EMIT resumeFallbackRequested(entry);
        return;
    }
    auto task = resumeTask(entry);
    Q_UNUSED(task);
}

QCoro::Task<void> HistoryController::resumeTask(api::HistoryEntry entry)
{
    const auto myEpoch = ++m_resumeEpoch;

    Q_EMIT statusMessage(
        i18nc("@info:status", "Resuming \u201c%1\u201d\u2026",
            entry.title.isEmpty()
                ? entry.lastStream.releaseName
                : entry.title),
        0);

    if (!m_torrentio) {
        Q_EMIT resumeFallbackRequested(entry);
        co_return;
    }

    auto opts = m_settings.torrentioOptions();
    opts.realDebridToken = m_rdToken;

    QList<api::Stream> streams;
    try {
        const auto streamId = entry.key.storageKey();
        streams = co_await m_torrentio->streams(
            entry.key.kind, streamId, opts);
    } catch (const std::exception& e) {
        if (myEpoch != m_resumeEpoch) {
            co_return;
        }
        qCWarning(KINEMA)
            << "HistoryController: torrentio fetch failed for resume:"
            << core::describeError(e, "resume/torrentio");
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "Could not reach Torrentio to resume \u201c%1\u201d.",
                entry.title),
            6000);
        Q_EMIT resumeFallbackRequested(entry);
        co_return;
    }

    if (myEpoch != m_resumeEpoch) {
        co_return;
    }

    // Match by infoHash. Require a playable direct URL; without one
    // we can't hand off to the player anyway.
    const api::Stream* hit = nullptr;
    for (const auto& s : streams) {
        if (entry.lastStream.matches(s) && !s.directUrl.isEmpty()) {
            hit = &s;
            break;
        }
    }

    if (!hit) {
        qCInfo(KINEMA).nospace()
            << "HistoryController: saved release not in current "
               "Torrentio response for " << entry.key.storageKey()
            << " (hash=\"" << entry.lastStream.infoHash
            << "\", name=\"" << entry.lastStream.releaseName
            << "\", " << streams.size() << " candidates)";
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "Saved release of \u201c%1\u201d is no longer "
                "available.",
                entry.title),
            6000);
        Q_EMIT resumeFallbackRequested(entry);
        co_return;
    }

    if (!m_actions) {
        qCWarning(KINEMA)
            << "HistoryController: no StreamActions wired; "
               "cannot dispatch resume";
        Q_EMIT resumeFallbackRequested(entry);
        co_return;
    }

    // Build the PlaybackContext from the history row (not the fresh
    // stream) so fields like `title` and `poster` retain what the
    // user originally saw.
    api::PlaybackContext ctx;
    ctx.key = entry.key;
    ctx.title = entry.title;
    ctx.seriesTitle = entry.seriesTitle;
    ctx.episodeTitle = entry.episodeTitle;
    ctx.poster = entry.poster;
    // streamRef and resumeSeconds are filled by StreamActions::play.

    m_actions->play(*hit, ctx);
}

void HistoryController::onFileLoaded()
{
    if (m_pending) {
        m_active = m_pending;
        m_pending.reset();
    }
    m_lastPosition = 0.0;
    m_lastPersistedPosition = 0.0;
}

void HistoryController::onEndOfFile(const QString& /*reason*/)
{
    if (!m_active) {
        return;
    }
    // Persist the final position synchronously.
    persistActive(/*force=*/true);
    m_active.reset();
    m_pending.reset();
    m_lastPosition = 0.0;
    m_duration = 0.0;
    m_lastPersistedPosition = 0.0;
}

void HistoryController::onDurationChanged(double seconds)
{
    if (seconds > 0.0) {
        m_duration = seconds;
    }
}

void HistoryController::onPositionChanged(double seconds)
{
    if (!m_active || seconds < 0.0) {
        return;
    }
    m_lastPosition = seconds;

    if (m_duration <= 0.0) {
        return;
    }
    if (seconds / m_duration < kMinProgressFraction) {
        return;
    }
    if (qAbs(seconds - m_lastPersistedPosition) < kPersistIntervalSec) {
        return;
    }
    persistActive(/*force=*/false);
}

void HistoryController::onPersistTick()
{
    persistActive(/*force=*/true);
}

void HistoryController::persistActive(bool force)
{
    if (!m_active) {
        return;
    }
    if (!force && qAbs(m_lastPosition - m_lastPersistedPosition)
            < kPersistIntervalSec) {
        return;
    }

    api::HistoryEntry e;
    e.key = m_active->key;
    e.title = m_active->title;
    e.seriesTitle = m_active->seriesTitle;
    e.episodeTitle = m_active->episodeTitle;
    e.poster = m_active->poster;
    e.lastStream = m_active->streamRef;
    e.positionSec = m_lastPosition;
    e.durationSec = m_duration;
    e.lastWatchedAt = QDateTime::currentDateTimeUtc();

#ifdef KINEMA_HAVE_LIBMPV
    if (m_player && m_player->mpv()) {
        const auto& tracks = m_player->mpv()->trackList();
        e.rememberedAudioLang = selectedAudioLang(tracks);
        e.rememberedSubtitleLang = selectedSubtitleLang(tracks);
    }
#endif

    m_store.record(e);
    m_lastPersistedPosition = m_lastPosition;
}

} // namespace kinema::controllers
