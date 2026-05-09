// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/PlayQueueController.h"

#include "api/TorrentioClient.h"
#include "config/AppSettings.h"
#include "core/HttpErrorPresenter.h"
#include "core/PlayQueueStore.h"
#include "kinema_debug.h"
#include "services/StreamActions.h"

#include <KLocalizedString>

#include <QDateTime>

namespace kinema::controllers {

namespace {

bool isPlayable(const api::Stream& s)
{
    return !s.directUrl.isEmpty() || !s.infoHash.isEmpty();
}

} // namespace

PlayQueueController::PlayQueueController(core::PlayQueueStore& store,
    api::TorrentioClient& torrentio,
    services::StreamActions& actions,
    const config::AppSettings& settings,
    const QString& rdTokenRef,
    QObject* parent)
    : QObject(parent)
    , m_store(store)
    , m_torrentio(torrentio)
    , m_actions(actions)
    , m_settings(settings)
    , m_rdToken(rdTokenRef)
{
    // Hydrate from disk. Persisted rows always reload as Pending;
    // active state is per-session.
    Q_EMIT itemsAboutToReset();
    m_items = m_store.loadAll();
    for (auto& it : m_items) {
        it.status = api::QueueItem::Status::Pending;
    }
    Q_EMIT itemsReset();
}

PlayQueueController::~PlayQueueController() = default;

// ------------------------------------------------------------------
//  Public slots
// ------------------------------------------------------------------

void PlayQueueController::playNow(const api::Stream& s,
    const api::PlaybackContext& ctx)
{
    if (!isPlayable(s)) {
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "This release has no playable URL or magnet."),
            5000);
        return;
    }

    const int existing = indexOfKey(ctx.key);
    if (existing == m_activeIndex && existing >= 0) {
        // Already the active row — just refresh the streamRef and
        // re-dispatch. We do not move it: with the played group
        // above the active row, "position 0" is no longer the
        // natural home of the active item.
        replaceRowFromStream(existing, s, ctx);
        dispatchActiveItem();
        return;
    }

    if (existing >= 0) {
        removeRowForReinsert(existing);
    }

    // Demote any previously-active row.
    if (m_activeIndex >= 0 && m_activeIndex < m_items.size()) {
        m_items[m_activeIndex].status = api::QueueItem::Status::Pending;
        Q_EMIT itemChanged(m_activeIndex);
    }

    // Insert at the active slot when something is active so the new
    // item slots in immediately above the previously-active row, or
    // at the head of the pending range when idle. This keeps the
    // played group above the active row instead of being shoved
    // beneath a freshly-inserted row.
    const int insertIdx = (m_activeIndex >= 0)
        ? m_activeIndex
        : firstPendingIndex();

    auto item = buildItem(s, ctx);
    const int actual = insertAt(std::move(item), insertIdx);
    setActiveIndex(actual);
    dispatchActiveItem();
}

void PlayQueueController::playNext(const api::Stream& s,
    const api::PlaybackContext& ctx)
{
    if (!isPlayable(s)) {
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "This release has no playable URL or magnet."),
            5000);
        return;
    }

    const int existing = indexOfKey(ctx.key);
    const QString title = ctx.title.isEmpty() ? s.releaseName : ctx.title;
    if (existing == m_activeIndex && existing >= 0) {
        Q_EMIT statusMessage(
            i18nc("@info:status", "Already playing: \u201c%1\u201d", title),
            2500);
        return;
    }
    if (existing >= 0) {
        removeRowForReinsert(existing);
    }

    auto item = buildItem(s, ctx);
    const int insertIdx = (m_activeIndex >= 0) ? m_activeIndex + 1 : 0;
    insertAt(std::move(item), insertIdx);

    Q_EMIT statusMessage(
        i18nc("@info:status", "Will play next: \u201c%1\u201d", title),
        2500);
}

void PlayQueueController::enqueue(const api::Stream& s,
    const api::PlaybackContext& ctx)
{
    if (!isPlayable(s)) {
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "This release has no playable URL or magnet."),
            5000);
        return;
    }

    const int existing = indexOfKey(ctx.key);
    const QString title = ctx.title.isEmpty() ? s.releaseName : ctx.title;
    if (existing == m_activeIndex && existing >= 0) {
        Q_EMIT statusMessage(
            i18nc("@info:status", "Already playing: \u201c%1\u201d", title),
            2500);
        return;
    }
    if (existing >= 0) {
        removeRowForReinsert(existing);
    }

    auto item = buildItem(s, ctx);
    insertAt(std::move(item), m_items.size());

    Q_EMIT statusMessage(
        i18nc("@info:status", "Added to queue: \u201c%1\u201d", title),
        2500);
}

void PlayQueueController::playAt(int index)
{
    if (index < 0 || index >= m_items.size()) {
        return;
    }
    if (index != m_activeIndex
        && m_activeIndex >= 0 && m_activeIndex < m_items.size()) {
        m_items[m_activeIndex].status = api::QueueItem::Status::Pending;
        Q_EMIT itemChanged(m_activeIndex);
    }

    setActiveIndex(index);
    dispatchActiveItem();
}

void PlayQueueController::removeAt(int index)
{
    if (index < 0 || index >= m_items.size()) {
        return;
    }
    const bool wasActive = (index == m_activeIndex);

    Q_EMIT itemAboutToBeRemoved(index);
    m_items.removeAt(index);
    Q_EMIT itemRemoved(index);

    if (wasActive) {
        // Active row is gone. Pin to the same slot in the post-
        // remove list so the next pending row takes its place;
        // dispatch playback for it.
        ++m_resolveEpoch; // discard any in-flight resolve
        m_userClosePending = false;
        if (index < m_items.size()) {
            setActiveIndex(index);
            dispatchActiveItem();
        } else {
            setActiveIndex(-1);
            // No more items - tell the host to hide the player.
            Q_EMIT windowCloseRequested();
        }
    } else if (m_activeIndex > index) {
        setActiveIndex(m_activeIndex - 1);
    }
    persist();
}

void PlayQueueController::moveTo(int from, int to)
{
    const int n = m_items.size();
    if (from < 0 || from >= n) {
        return;
    }
    if (to < 0) {
        to = 0;
    }
    if (to >= n) {
        to = n - 1;
    }
    if (from == to) {
        return;
    }

    Q_EMIT itemAboutToBeMoved(from, to);
    m_items.move(from, to);

    // Track the active index across the move. QList::move(from, to)
    // takes an item out of `from` and inserts it at `to` in the
    // resulting list. Keep `m_activeIndex` pointing at the same row
    // it pointed to before.
    int newActive = m_activeIndex;
    if (m_activeIndex == from) {
        newActive = to;
    } else if (from < m_activeIndex && m_activeIndex <= to) {
        newActive = m_activeIndex - 1;
    } else if (to <= m_activeIndex && m_activeIndex < from) {
        newActive = m_activeIndex + 1;
    }

    Q_EMIT itemMoved(from, to);
    setActiveIndex(newActive);
    if (m_reordering) {
        m_reorderDirty = true;
    } else {
        persist();
    }
}

void PlayQueueController::beginReorder()
{
    m_reordering = true;
    m_reorderDirty = false;
}

void PlayQueueController::endReorder()
{
    m_reordering = false;
    if (m_reorderDirty) {
        m_reorderDirty = false;
        persist();
    }
}

void PlayQueueController::clearAll()
{
    if (m_items.isEmpty()) {
        return;
    }
    Q_EMIT itemsAboutToReset();
    m_items.clear();
    setActiveIndex(-1);
    ++m_resolveEpoch;
    m_userClosePending = false;
    Q_EMIT itemsReset();
    m_store.clear();
}

void PlayQueueController::clearAllExceptActive()
{
    if (m_items.isEmpty()) {
        return;
    }
    if (m_activeIndex < 0 || m_activeIndex >= m_items.size()) {
        clearAll();
        return;
    }

    const auto active = m_items.at(m_activeIndex);

    Q_EMIT itemsAboutToReset();
    m_items = { active };
    m_items[0].order = 0;
    setActiveIndex(0);
    m_userClosePending = false;
    Q_EMIT itemsReset();
    persist();

    Q_EMIT statusMessage(
        i18nc("@info:status",
            "Cleared the queue and kept only the current item."),
        3000);
}

void PlayQueueController::clearPlayed()
{
    if (m_items.isEmpty()) {
        return;
    }
    bool changed = false;
    // Walk from the end so indices remain stable for the rows we
    // haven't visited yet.
    for (int i = m_items.size() - 1; i >= 0; --i) {
        if (m_items[i].status != api::QueueItem::Status::Played) {
            continue;
        }
        Q_EMIT itemAboutToBeRemoved(i);
        m_items.removeAt(i);
        Q_EMIT itemRemoved(i);
        if (m_activeIndex > i) {
            setActiveIndex(m_activeIndex - 1);
        }
        changed = true;
    }
    if (changed) {
        Q_EMIT statusMessage(
            i18nc("@info:status", "Cleared played items."),
            3000);
        persist();
    }
}

void PlayQueueController::onPlayerEndOfFile(const QString& reason,
    const api::PlaybackContext& ctx)
{
    Q_UNUSED(ctx);

    // User-close takes precedence: mpv emits an EOF ("stop") right
    // after we tell it to stop in `closeEvent`. Consume it.
    if (m_userClosePending) {
        m_userClosePending = false;
        if (m_activeIndex >= 0 && m_activeIndex < m_items.size()) {
            m_items[m_activeIndex].status =
                api::QueueItem::Status::Pending;
            Q_EMIT itemChanged(m_activeIndex);
        }
        return;
    }

    if (m_activeIndex < 0 || m_activeIndex >= m_items.size()) {
        return;
    }

    const bool isError = reason == QStringLiteral("error");
    if (isError) {
        // Mark Failed and skip-and-advance.
        m_items[m_activeIndex].status =
            api::QueueItem::Status::Failed;
        Q_EMIT itemChanged(m_activeIndex);
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "Skipped \u201c%1\u201d - playback failed.",
                m_items[m_activeIndex].title),
            5000);

        // Find the next playable row after this one and dispatch.
        // Skip over both `Failed` (already known-broken) and
        // `Played` (already finished; the user has to click them
        // explicitly to replay).
        const int from = m_activeIndex;
        int next = -1;
        for (int i = from + 1; i < m_items.size(); ++i) {
            const auto st = m_items[i].status;
            if (st != api::QueueItem::Status::Failed
                && st != api::QueueItem::Status::Played) {
                next = i;
                break;
            }
        }
        if (next >= 0) {
            setActiveIndex(next);
            dispatchActiveItem();
        } else {
            setActiveIndex(-1);
            Q_EMIT windowCloseRequested();
        }
        return;
    }

    // Clean end-of-file (eof / empty reason / stop without user-close):
    // active item was watched to completion. Mark it `Played` in
    // place — the played group sits above the active row so the
    // user can replay it from the queue page or via `Previous`.
    const int oldIdx = m_activeIndex;
    m_items[oldIdx].status = api::QueueItem::Status::Played;
    Q_EMIT itemChanged(oldIdx);
    ++m_resolveEpoch;
    m_userClosePending = false;

    // Find the next playable row.
    int next = -1;
    for (int i = oldIdx + 1; i < m_items.size(); ++i) {
        const auto st = m_items[i].status;
        if (st != api::QueueItem::Status::Played
            && st != api::QueueItem::Status::Failed) {
            next = i;
            break;
        }
    }

    if (next >= 0) {
        setActiveIndex(next);
    } else {
        setActiveIndex(-1);
    }

    // Trim the played group (oldest rows first); may shift indices.
    enforcePlayedCap();

    if (m_activeIndex >= 0) {
        dispatchActiveItem();
    } else {
        Q_EMIT statusMessage(
            i18nc("@info:status", "Queue finished."), 4000);
        Q_EMIT windowCloseRequested();
    }
    persist();
}

void PlayQueueController::onPlayerUserClosed()
{
    m_userClosePending = true;
    if (m_activeIndex >= 0 && m_activeIndex < m_items.size()) {
        m_items[m_activeIndex].status =
            api::QueueItem::Status::Pending;
        Q_EMIT itemChanged(m_activeIndex);
    }
}

void PlayQueueController::playPreviousItem()
{
    if (m_activeIndex > 0) {
        playAt(m_activeIndex - 1);
    }
}

void PlayQueueController::playNextItem()
{
    if (m_activeIndex >= 0 && (m_activeIndex + 1) < m_items.size()) {
        playAt(m_activeIndex + 1);
    }
}

void PlayQueueController::retryFailed(int index)
{
    if (index < 0 || index >= m_items.size()) {
        return;
    }
    auto& it = m_items[index];
    if (it.status != api::QueueItem::Status::Failed) {
        return;
    }
    it.status = api::QueueItem::Status::Pending;
    Q_EMIT itemChanged(index);

    if (m_activeIndex == -1 || m_activeIndex == index) {
        if (m_activeIndex != index) {
            setActiveIndex(index);
        }
        dispatchActiveItem();
    }
}

// ------------------------------------------------------------------
//  Resolution + dispatch
// ------------------------------------------------------------------

QCoro::Task<void> PlayQueueController::startActiveItem()
{
    const auto myEpoch = m_resolveEpoch;
    if (m_activeIndex < 0 || m_activeIndex >= m_items.size()) {
        co_return;
    }

    // Snapshot the active item by value. The list could mutate
    // under us while a Torrentio fetch is in-flight; we'll re-check
    // the epoch and the current active index before applying any
    // state changes.
    api::QueueItem item = m_items[m_activeIndex];

    // Mark Active and notify.
    m_items[m_activeIndex].status = api::QueueItem::Status::Active;
    Q_EMIT itemChanged(m_activeIndex);

    QUrl url = m_rdToken.isEmpty() ? QUrl {} : item.cachedDirectUrl;
    if (!url.isEmpty()) {
        // Consume the cached URL so a later pass re-resolves cleanly.
        m_items[m_activeIndex].cachedDirectUrl.clear();
    } else {
        if (m_rdToken.isEmpty() && !m_items[m_activeIndex].cachedDirectUrl.isEmpty()) {
            m_items[m_activeIndex].cachedDirectUrl.clear();
        }
        // Re-resolve via Torrentio + match-by-infoHash, mirroring the
        // pattern HistoryController::resumeFromHistory uses.
        auto opts = m_settings.torrentioOptions();
        opts.realDebridToken = m_rdToken;

        QList<api::Stream> streams;
        try {
            const auto streamId = item.key.storageKey();
            streams = co_await m_torrentio.streams(
                item.key.kind, streamId, opts);
        } catch (const std::exception& e) {
            if (myEpoch != m_resolveEpoch) {
                co_return;
            }
            qCWarning(KINEMA)
                << "PlayQueueController: torrentio fetch failed:"
                << core::describeError(e, "queue/torrentio");
            // Mark Failed; step 4 will replace this with skip-and-advance.
            if (m_activeIndex >= 0
                && m_activeIndex < m_items.size()) {
                m_items[m_activeIndex].status =
                    api::QueueItem::Status::Failed;
                Q_EMIT itemChanged(m_activeIndex);
            }
            Q_EMIT statusMessage(
                i18nc("@info:status",
                    "Could not reach Torrentio for \u201c%1\u201d.",
                    item.title),
                6000);
            co_return;
        }

        if (myEpoch != m_resolveEpoch) {
            co_return;
        }

        const api::Stream* hit = nullptr;
        for (const auto& s : streams) {
            if (item.streamRef.matches(s) && isPlayable(s)) {
                hit = &s;
                if (!s.directUrl.isEmpty()) {
                    break;
                }
            }
        }

        if (!hit) {
            qCInfo(KINEMA).nospace()
                << "PlayQueueController: queued release not in current "
                   "Torrentio response for " << item.key.storageKey()
                << " (hash=\"" << item.streamRef.infoHash << "\")";
            if (m_activeIndex >= 0
                && m_activeIndex < m_items.size()) {
                m_items[m_activeIndex].status =
                    api::QueueItem::Status::Failed;
                Q_EMIT itemChanged(m_activeIndex);
            }
            Q_EMIT statusMessage(
                i18nc("@info:status",
                    "Queued release of \u201c%1\u201d is no longer "
                    "available.",
                    item.title),
                6000);
            co_return;
        }
        url = hit->directUrl;
        if (url.isEmpty()) {
            const auto stream = *hit;
            auto ctx = toContext(item);
            m_actions.play(stream, ctx);
            co_return;
        }
    }

    if (myEpoch != m_resolveEpoch) {
        co_return;
    }

    // Hand off through StreamActions::play. It will: build streamRef
    // from the stream, look up resume seconds, fire onPlayStarting,
    // and dispatch through PlayerLauncher (external) or
    // embeddedRequested (libmpv).
    const auto stream = toStream(item, url);
    auto ctx = toContext(item);
    m_actions.play(stream, ctx);
}

// ------------------------------------------------------------------
//  Helpers
// ------------------------------------------------------------------

int PlayQueueController::indexOfKey(const api::PlaybackKey& key) const
{
    for (int i = 0; i < m_items.size(); ++i) {
        if (m_items[i].key == key) {
            return i;
        }
    }
    return -1;
}

void PlayQueueController::replaceRowFromStream(int index,
    const api::Stream& s, const api::PlaybackContext& ctx)
{
    if (index < 0 || index >= m_items.size()) {
        return;
    }

    auto& item = m_items[index];
    item.key = ctx.key;
    item.title = ctx.title.isEmpty()
        ? (s.releaseName.isEmpty() ? s.qualityLabel : s.releaseName)
        : ctx.title;
    item.seriesTitle = ctx.seriesTitle;
    item.episodeTitle = ctx.episodeTitle;
    item.poster = ctx.poster;
    item.streamRef = api::HistoryStreamRef::fromStream(s);
    item.cachedDirectUrl = s.directUrl;
    item.addedAt = QDateTime::currentDateTimeUtc();
    item.status = api::QueueItem::Status::Pending;
    Q_EMIT itemChanged(index);
    persist();
}

void PlayQueueController::removeRowForReinsert(int index)
{
    if (index < 0 || index >= m_items.size()) {
        return;
    }

    Q_EMIT itemAboutToBeRemoved(index);
    m_items.removeAt(index);
    Q_EMIT itemRemoved(index);

    if (m_activeIndex > index) {
        setActiveIndex(m_activeIndex - 1);
    }
    persist();
}

api::QueueItem PlayQueueController::buildItem(const api::Stream& s,
    const api::PlaybackContext& ctx)
{
    api::QueueItem it;
    it.key = ctx.key;
    it.title = ctx.title.isEmpty()
        ? (s.releaseName.isEmpty() ? s.qualityLabel : s.releaseName)
        : ctx.title;
    it.seriesTitle = ctx.seriesTitle;
    it.episodeTitle = ctx.episodeTitle;
    it.poster = ctx.poster;
    it.streamRef = api::HistoryStreamRef::fromStream(s);
    it.cachedDirectUrl = s.directUrl;
    it.addedAt = QDateTime::currentDateTimeUtc();
    return it;
}

api::Stream PlayQueueController::toStream(const api::QueueItem& item,
    const QUrl& url)
{
    api::Stream s;
    s.directUrl = url;
    s.infoHash = item.streamRef.infoHash;
    s.releaseName = item.streamRef.releaseName;
    s.resolution = item.streamRef.resolution;
    s.qualityLabel = item.streamRef.qualityLabel;
    s.sizeBytes = item.streamRef.sizeBytes;
    s.provider = item.streamRef.provider;
    return s;
}

api::PlaybackContext PlayQueueController::toContext(
    const api::QueueItem& item)
{
    api::PlaybackContext ctx;
    ctx.key = item.key;
    ctx.title = item.title;
    ctx.seriesTitle = item.seriesTitle;
    ctx.episodeTitle = item.episodeTitle;
    ctx.poster = item.poster;
    // streamRef and resumeSeconds are filled by StreamActions::play.
    return ctx;
}

int PlayQueueController::insertAt(api::QueueItem item, int atIndex)
{
    const int n = m_items.size();
    if (atIndex < 0) {
        atIndex = 0;
    } else if (atIndex > n) {
        atIndex = n;
    }
    item.order = atIndex; // store will renumber on persist anyway

    Q_EMIT itemAboutToBeInserted(atIndex);
    m_items.insert(atIndex, std::move(item));
    Q_EMIT itemInserted(atIndex);

    // Inserting at or before the active index pushes it down. Emit
    // after the insert notification has completed so views never see
    // `dataChanged` while an insert operation is still open.
    if (m_activeIndex >= atIndex) {
        setActiveIndex(m_activeIndex + 1);
    }
    persist();
    return atIndex;
}

int PlayQueueController::firstPendingIndex() const
{
    for (int i = 0; i < m_items.size(); ++i) {
        if (m_items[i].status != api::QueueItem::Status::Played) {
            return i;
        }
    }
    return m_items.size();
}

void PlayQueueController::enforcePlayedCap()
{
    constexpr int kPlayedCap = 20;

    int played = 0;
    for (const auto& it : m_items) {
        if (it.status == api::QueueItem::Status::Played) {
            ++played;
        }
    }
    while (played > kPlayedCap) {
        // Drop the oldest played row (lowest index, since natural
        // EOF appends played status at the formerly-active slot,
        // which is always lower than the new active slot).
        for (int i = 0; i < m_items.size(); ++i) {
            if (m_items[i].status
                != api::QueueItem::Status::Played) {
                continue;
            }
            Q_EMIT itemAboutToBeRemoved(i);
            m_items.removeAt(i);
            Q_EMIT itemRemoved(i);
            if (m_activeIndex > i) {
                setActiveIndex(m_activeIndex - 1);
            }
            --played;
            break;
        }
    }
}

void PlayQueueController::persist()
{
    // Filter out `Played` rows so they do not resurrect across
    // restarts (matching the design that `Played` is session-only).
    // Keep an index map from the persisted snapshot back to
    // `m_items` so we can copy assigned ids onto the right rows
    // without disturbing played entries.
    QList<api::QueueItem> toPersist;
    QList<int> indexMap;
    toPersist.reserve(m_items.size());
    indexMap.reserve(m_items.size());
    for (int i = 0; i < m_items.size(); ++i) {
        if (m_items[i].status == api::QueueItem::Status::Played) {
            continue;
        }
        toPersist.append(m_items[i]);
        indexMap.append(i);
    }

    const auto written = m_store.replaceAll(toPersist);
    if (written.size() != toPersist.size()) {
        qCWarning(KINEMA)
            << "PlayQueueController: persist replaceAll size mismatch ("
            << written.size() << "vs" << toPersist.size() << ")";
        return;
    }
    for (int j = 0; j < written.size(); ++j) {
        const int i = indexMap[j];
        m_items[i].id = written[j].id;
        m_items[i].order = written[j].order;
        if (!m_items[i].addedAt.isValid()) {
            m_items[i].addedAt = written[j].addedAt;
        }
    }
}

void PlayQueueController::setActiveIndex(int index)
{
    if (index == m_activeIndex) {
        return;
    }
    m_activeIndex = index;
    Q_EMIT activeIndexChanged(index);
}

void PlayQueueController::dispatchActiveItem()
{
    ++m_resolveEpoch;
    m_userClosePending = false;
    auto task = startActiveItem();
    Q_UNUSED(task);
}

} // namespace kinema::controllers
