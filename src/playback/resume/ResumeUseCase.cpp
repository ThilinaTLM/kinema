// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/resume/ResumeUseCase.h"

#include "core/io/HttpErrorPresenter.h"
#include "kinema_log_controller.h"
#include "playback/history/HistoryQueryService.h"
#include "playback/policy/ResumePolicy.h"
#include "playback/ports/PlaybackHistoryRepository.h"
#include "playback/ports/StreamIndexerPort.h"
#include "playback/progress/PlaybackProgressProjector.h"
#include "services/StreamActions.h"

#include <KLocalizedString>

namespace kinema::playback::resume {

ResumeUseCase::ResumeUseCase(
    playback::history::HistoryQueryService& queryService,
    playback::progress::PlaybackProgressProjector& projector,
    ports::StreamIndexerPort& indexer,
    ports::PlaybackHistoryRepository& historyRepo,
    services::StreamActions& actions,
    QObject* parent)
    : QObject(parent)
    , m_queryService(queryService)
    , m_projector(projector)
    , m_indexer(indexer)
    , m_historyRepo(historyRepo)
    , m_actions(actions)
{
}

ResumeUseCase::~ResumeUseCase() = default;

std::optional<qint64> ResumeUseCase::resumeSecondsFor(
    const domain::PlaybackKey& key) const
{
    if (!key.isValid()) {
        return std::nullopt;
    }
    policy::ResumeInputs in;
    in.key = key;
    // Live in-memory position is authoritative when the projector
    // is still tracking the same key (mid-session stream swap).
    if (m_projector.hasActiveContext()) {
        const auto& ctx = m_projector.activeContext();
        if (ctx.has_value() && ctx->key == key) {
            in.activeSessionMatches = true;
            in.activeSessionPositionSec = m_projector.lastPosition();
        }
    }
    in.storedEntry = m_queryService.find(key);
    return policy::resumeSecondsFor(in);
}

void ResumeUseCase::resume(const domain::HistoryEntry& entry)
{
    if (!entry.key.isValid()) {
        return;
    }
    // We need at least one of (infoHash, releaseName) to re-resolve
    // the saved release against the indexer. Missing both means the
    // entry predates the stream-tracking code or was recorded
    // without a chosen release — no way to find it again.
    if (entry.lastStream.infoHash.isEmpty()
        && entry.lastStream.releaseName.isEmpty()) {
        qCInfo(KINEMA_CONTROLLER)
            << "ResumeUseCase: cannot resume" << entry.key.storageKey()
            << "— no saved release reference (delete + replay to recover)";
        Q_EMIT resumeFallbackRequested(entry);
        return;
    }
    auto task = resumeTask(entry);
    Q_UNUSED(task);
}

void ResumeUseCase::removeEntry(const domain::HistoryEntry& entry)
{
    if (!entry.key.isValid()) {
        return;
    }
    m_historyRepo.remove(entry.key);
}

QCoro::Task<void> ResumeUseCase::resumeTask(domain::HistoryEntry entry)
{
    const auto myEpoch = ++m_resumeEpoch;

    Q_EMIT statusMessage(
        i18nc("@info:status", "Resuming \u201c%1\u201d\u2026",
            entry.title.isEmpty()
                ? entry.lastStream.releaseName
                : entry.title),
        0);

    QList<domain::Stream> streams;
    try {
        const auto streamId = entry.key.storageKey();
        streams = co_await m_indexer.streamsFor(entry.key.kind, streamId);
    } catch (const std::exception& e) {
        if (myEpoch != m_resumeEpoch) {
            co_return;
        }
        qCWarning(KINEMA_CONTROLLER)
            << "ResumeUseCase: indexer fetch failed for resume:"
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
            << "ResumeUseCase: saved release not in current "
               "indexer response for " << entry.key.storageKey()
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
    m_actions.play(*hit, ctx);
}

} // namespace kinema::playback::resume
