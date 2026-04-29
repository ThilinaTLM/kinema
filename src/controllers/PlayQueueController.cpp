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
    if (s.directUrl.isEmpty()) {
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "This release has no direct URL \u2014 use Real-Debrid for one-click play."),
            5000);
        return;
    }

    // Demote any previously-active row.
    if (m_activeIndex >= 0 && m_activeIndex < m_items.size()) {
        m_items[m_activeIndex].status = api::QueueItem::Status::Pending;
        Q_EMIT itemChanged(m_activeIndex);
    }

    auto item = buildItem(s, ctx);
    insertAt(std::move(item), 0);
    setActiveIndex(0);

    // Bump epoch + kick off resolution.
    ++m_resolveEpoch;
    auto task = startActiveItem();
    Q_UNUSED(task);
}

void PlayQueueController::playNext(const api::Stream& s,
    const api::PlaybackContext& ctx)
{
    if (s.directUrl.isEmpty()) {
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "This release has no direct URL \u2014 use Real-Debrid for one-click play."),
            5000);
        return;
    }

    auto item = buildItem(s, ctx);
    const int insertIdx = (m_activeIndex >= 0)
        ? m_activeIndex + 1
        : 0;
    const int placed = insertAt(std::move(item), insertIdx);

    // Inserting at or before the active index would shift it; this
    // path always inserts strictly after the active row, so the
    // active index is unaffected.
    Q_UNUSED(placed);

    Q_EMIT statusMessage(
        i18nc("@info:status", "Will play next: \u201c%1\u201d",
            ctx.title.isEmpty() ? s.releaseName : ctx.title),
        2500);
}

void PlayQueueController::enqueue(const api::Stream& s,
    const api::PlaybackContext& ctx)
{
    if (s.directUrl.isEmpty()) {
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "This release has no direct URL \u2014 use Real-Debrid for one-click play."),
            5000);
        return;
    }

    auto item = buildItem(s, ctx);
    insertAt(std::move(item), m_items.size());

    Q_EMIT statusMessage(
        i18nc("@info:status", "Added to queue: \u201c%1\u201d",
            ctx.title.isEmpty() ? s.releaseName : ctx.title),
        2500);
}

void PlayQueueController::playAt(int index)
{
    if (index < 0 || index >= m_items.size()) {
        return;
    }
    if (index == m_activeIndex) {
        // Re-dispatch the same row \u2014 user clicked the active row.
        ++m_resolveEpoch;
        auto task = startActiveItem();
        Q_UNUSED(task);
        return;
    }

    if (m_activeIndex >= 0 && m_activeIndex < m_items.size()) {
        m_items[m_activeIndex].status = api::QueueItem::Status::Pending;
        Q_EMIT itemChanged(m_activeIndex);
    }

    setActiveIndex(index);
    ++m_resolveEpoch;
    auto task = startActiveItem();
    Q_UNUSED(task);
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
        if (index < m_items.size()) {
            setActiveIndex(index);
            auto task = startActiveItem();
            Q_UNUSED(task);
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
    persist();
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
    Q_EMIT itemsReset();
    m_store.clear();
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

        // Find the next non-Failed row after this one and dispatch.
        const int from = m_activeIndex;
        int next = -1;
        for (int i = from + 1; i < m_items.size(); ++i) {
            if (m_items[i].status != api::QueueItem::Status::Failed) {
                next = i;
                break;
            }
        }
        if (next >= 0) {
            setActiveIndex(next);
            ++m_resolveEpoch;
            auto task = startActiveItem();
            Q_UNUSED(task);
        } else {
            setActiveIndex(-1);
            Q_EMIT windowCloseRequested();
        }
        return;
    }

    // Clean end-of-file (eof / empty reason / stop without user-close):
    // active item was watched to completion. Remove it and advance.
    const int oldIdx = m_activeIndex;
    Q_EMIT itemAboutToBeRemoved(oldIdx);
    m_items.removeAt(oldIdx);
    Q_EMIT itemRemoved(oldIdx);
    ++m_resolveEpoch;

    if (oldIdx < m_items.size()) {
        setActiveIndex(oldIdx);
        auto task = startActiveItem();
        Q_UNUSED(task);
    } else {
        setActiveIndex(-1);
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
        ++m_resolveEpoch;
        auto task = startActiveItem();
        Q_UNUSED(task);
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

    QUrl url = item.cachedDirectUrl;
    if (!url.isEmpty()) {
        // Consume the cached URL so a later pass re-resolves cleanly.
        m_items[m_activeIndex].cachedDirectUrl.clear();
    } else {
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
            if (item.streamRef.matches(s) && !s.directUrl.isEmpty()) {
                hit = &s;
                break;
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

void PlayQueueController::persist()
{
    // Renumber and write back. The store's replaceAll backfills ids
    // for any 0-id rows; copy them back so the in-memory list stays
    // canonical.
    const auto written = m_store.replaceAll(m_items);
    if (written.size() != m_items.size()) {
        qCWarning(KINEMA)
            << "PlayQueueController: persist replaceAll size mismatch ("
            << written.size() << "vs" << m_items.size() << ")";
        return;
    }
    for (int i = 0; i < written.size(); ++i) {
        m_items[i].id = written[i].id;
        m_items[i].order = written[i].order;
        if (!m_items[i].addedAt.isValid()) {
            m_items[i].addedAt = written[i].addedAt;
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

} // namespace kinema::controllers
