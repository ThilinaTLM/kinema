// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/HistoryController.h"

#include "domain/Indexer.h"
#include "api/IndexerSelector.h"
#include "core/persistence/HistoryStore.h"
#include "core/io/HttpError.h"
#include "core/io/HttpErrorPresenter.h"
#include "kinema_log_controller.h"
#include "services/StreamActions.h"

#ifdef KINEMA_HAVE_LIBMPV

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
    api::IndexerSelector* indexers,
    const QString& rdTokenRef,
    QObject* parent)
    : QObject(parent)
    , m_store(store)
    , m_indexers(indexers)
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
    connect(m_player, &ui::player::PlayerWindow::chaptersChanged,
        this, &HistoryController::onChaptersChanged);
#else
    Q_UNUSED(window);
#endif
}

void HistoryController::setStreamActions(services::StreamActions* actions)
{
    m_actions = actions;
}

void HistoryController::onPlayStarting(const domain::PlaybackContext& ctx)
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
        m_activeChapters.clear();
    }

    m_pending = ctx;

    // The disk write that used to live here moved to
    // `playback::progress::PlaybackProgressProjector`, which seeds
    // the row from the `PlaybackRequested` event with identical
    // semantics (preserves existing progress on upsert; clears
    // `finished` so a re-watched finished row reappears in
    // Continue Watching). We still keep `m_pending` so the
    // in-memory resume helper (`resumeSecondsFor`) can return the
    // freshest live position for mid-session stream swaps until
    // ResumePolicy + ResumeUseCase fully own that path.
}

std::optional<qint64> HistoryController::resumeSecondsFor(
    const domain::PlaybackKey& key) const
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

std::optional<domain::HistoryEntry> HistoryController::find(
    const domain::PlaybackKey& key) const
{
    return m_store.find(key);
}

std::optional<domain::HistoryEntry> HistoryController::findLatestForMedia(
    domain::MediaKind kind, const QString& imdbId) const
{
    return m_store.findLatestForMedia(kind, imdbId);
}

QList<domain::HistoryEntry> HistoryController::continueWatching(
    int maxItems) const
{
    return m_store.continueWatching(maxItems);
}

void HistoryController::removeEntry(const domain::HistoryEntry& entry)
{
    if (!entry.key.isValid()) {
        return;
    }
    m_store.remove(entry.key);
}

void HistoryController::resumeFromHistory(const domain::HistoryEntry& entry)
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
        qCInfo(KINEMA_CONTROLLER)
            << "HistoryController: cannot resume" << entry.key.storageKey()
            << "— no saved release reference (delete + replay to recover)";
        Q_EMIT resumeFallbackRequested(entry);
        return;
    }
    auto task = resumeTask(entry);
    Q_UNUSED(task);
}

QCoro::Task<void> HistoryController::resumeTask(domain::HistoryEntry entry)
{
    const auto myEpoch = ++m_resumeEpoch;

    Q_EMIT statusMessage(
        i18nc("@info:status", "Resuming \u201c%1\u201d\u2026",
            entry.title.isEmpty()
                ? entry.lastStream.releaseName
                : entry.title),
        0);

    auto* indexer = m_indexers ? m_indexers->active() : nullptr;
    if (!indexer) {
        Q_EMIT resumeFallbackRequested(entry);
        co_return;
    }

    QList<domain::Stream> streams;
    try {
        const auto streamId = entry.key.storageKey();
        streams = co_await indexer->streams(entry.key.kind, streamId);
    } catch (const std::exception& e) {
        if (myEpoch != m_resumeEpoch) {
            co_return;
        }
        qCWarning(KINEMA_CONTROLLER)
            << "HistoryController: indexer fetch failed for resume:"
            << core::describeError(e, "resume/indexer");
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "Could not reach the stream indexer to resume \u201c%1\u201d.",
                entry.title),
            6000);
        Q_EMIT resumeFallbackRequested(entry);
        co_return;
    }

    if (myEpoch != m_resumeEpoch) {
        co_return;
    }

    // Match by infoHash/release. Prefer Real-Debrid direct URLs when
    // available, otherwise let StreamActions use built-in torrent
    // streaming for magnet-only rows.
    const domain::Stream* hit = nullptr;
    for (const auto& s : streams) {
        if (entry.lastStream.matches(s)
            && (!s.directUrl.isEmpty() || !s.infoHash.isEmpty())) {
            hit = &s;
            if (!s.directUrl.isEmpty()) {
                break;
            }
        }
    }

    if (!hit) {
        qCInfo(KINEMA_CONTROLLER).nospace()
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

    // Build the PlaybackContext from the history row (not the fresh
    // stream) so fields like `title` and `poster` retain what the
    // user originally saw.
    domain::PlaybackContext ctx;
    ctx.key = entry.key;
    ctx.title = entry.title;
    ctx.seriesTitle = entry.seriesTitle;
    ctx.episodeTitle = entry.episodeTitle;
    ctx.poster = entry.poster;
    ctx.backdrop = entry.backdrop;
    // streamRef and resumeSeconds are filled by StreamActions::play.

    if (!m_actions) {
        qCWarning(KINEMA_CONTROLLER)
            << "HistoryController: no StreamActions wired; "
               "cannot dispatch resume";
        Q_EMIT resumeFallbackRequested(entry);
        co_return;
    }

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
    // Fresh file → stale chapter list from a previous session must go.
    // mpv will re-emit `chapter-list` for the new file (possibly
    // empty, which is fine).
    m_activeChapters.clear();
}

void HistoryController::onEndOfFile(const QString& reason)
{
    if (!m_active) {
        return;
    }

    // Map mpv's reason string to the store's session-end taxonomy.
    // Anything we don't recognise is treated as an error so we never
    // auto-finish on an unexpected reason.
    core::HistoryStore::SessionEndReason mapped =
        core::HistoryStore::SessionEndReason::Error;
    if (reason == QLatin1String("eof")) {
        mapped = core::HistoryStore::SessionEndReason::NaturalEof;
    } else if (reason == QLatin1String("stop")
            || reason == QLatin1String("quit")) {
        mapped = core::HistoryStore::SessionEndReason::UserStop;
    }

    // The disk write that used to apply the session-end policy
    // moved to `playback::progress::PlaybackProgressProjector`,
    // which translates `PlaybackEnded` into
    // `recordSessionEnd(...)` with the same reason mapping and the
    // same chapter-derived credits-start hint. We still clear the
    // in-memory tracking here so `resumeSecondsFor` no longer
    // returns a stale live position for the just-ended session.
    Q_UNUSED(mapped);
    m_lastPersistedPosition = m_lastPosition;

    m_active.reset();
    m_pending.reset();
    m_lastPosition = 0.0;
    m_duration = 0.0;
    m_lastPersistedPosition = 0.0;
    m_activeChapters.clear();
}

void HistoryController::onChaptersChanged(
    const core::chapters::ChapterList& chapters)
{
    m_activeChapters = chapters;
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

    // Disk write moved to `PlaybackProgressProjector` (throttled
    // identically). Keep the in-memory cursor up to date so
    // `resumeSecondsFor` returns the fresh value during
    // mid-session stream swaps.
    m_lastPersistedPosition = m_lastPosition;
}

domain::HistoryEntry HistoryController::buildActiveEntry() const
{
    domain::HistoryEntry e;
    if (!m_active) {
        return e;
    }
    e.key = m_active->key;
    e.title = m_active->title;
    e.seriesTitle = m_active->seriesTitle;
    e.episodeTitle = m_active->episodeTitle;
    e.poster = m_active->poster;
    e.backdrop = m_active->backdrop;
    e.lastStream = m_active->streamRef;
    e.positionSec = m_lastPosition;
    e.durationSec = m_duration;
    e.lastWatchedAt = QDateTime::currentDateTimeUtc();

#ifdef KINEMA_HAVE_LIBMPV
    if (m_player) {
        const auto& tracks = m_player->trackList();
        e.rememberedAudioLang = selectedAudioLang(tracks);
        e.rememberedSubtitleLang = selectedSubtitleLang(tracks);
    }
#endif
    return e;
}

} // namespace kinema::controllers
