// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"
#include "api/PlaybackContext.h"
#include "api/QueueItem.h"

#include <QList>
#include <QObject>
#include <QString>

#include <QCoro/QCoroTask>

namespace kinema::api {
class TorrentioClient;
}

namespace kinema::config {
class AppSettings;
}

namespace kinema::core {
class PlayQueueStore;
}

namespace kinema::services {
class StreamActions;
}

namespace kinema::controllers {

/**
 * Owns the in-memory queue + drives playback for it. The single
 * source of truth for "what the user has lined up to watch" \u2014 every
 * play action (`Play now`, `Play next`, `Add to queue`) flows
 * through here.
 *
 * Lifecycle:
 *   - On construction, loads the persisted queue from
 *     `PlayQueueStore`. No item is marked active at startup;
 *     restoring playback is a deliberate user action.
 *   - Every mutation persists via `PlayQueueStore::replaceAll`,
 *     which renumbers `ord` atomically.
 *   - Stream URLs are NEVER persisted; on play we either use the
 *     in-memory `cachedDirectUrl` (set at enqueue time) or re-resolve
 *     by re-fetching Torrentio and matching by infoHash \u2014 the same
 *     pattern `HistoryController::resumeFromHistory` uses.
 *
 * Auto-advance is wired in step 4; step 2's controller does not yet
 * subscribe to `PlaybackController::endOfFile`. `playNow` and
 * `playAt` already drive a single-shot dispatch through
 * `StreamActions::play`.
 */
class PlayQueueController : public QObject
{
    Q_OBJECT
public:
    PlayQueueController(core::PlayQueueStore& store,
        api::TorrentioClient& torrentio,
        services::StreamActions& actions,
        const config::AppSettings& settings,
        const QString& rdTokenRef,
        QObject* parent = nullptr);

    ~PlayQueueController() override;

    /// Snapshot of the queue. Index-stable as long as the controller
    /// is not mutated; the row signals describe every change.
    const QList<api::QueueItem>& items() const noexcept { return m_items; }

    /// Index of the row currently in (or last handed to) the player.
    /// `-1` when nothing has been dispatched yet or the queue is empty.
    int activeIndex() const noexcept { return m_activeIndex; }

    bool isEmpty() const noexcept { return m_items.isEmpty(); }

public Q_SLOTS:
    /// "Play now": insert the stream at position 0 and start playing
    /// it. A previously-active item is demoted to slot 1 (status
    /// reverts to `Pending`); its mid-playback position is preserved
    /// by the existing `HistoryController` flow so resuming it later
    /// picks up where it was.
    void playNow(const api::Stream& s, const api::PlaybackContext& ctx);

    /// "Play next": insert immediately after the active item (or at
    /// position 0 if nothing is active). Does NOT preempt the active
    /// item; does NOT start playback by itself.
    void playNext(const api::Stream& s, const api::PlaybackContext& ctx);

    /// "Add to queue": append at the end. Never starts playback.
    void enqueue(const api::Stream& s, const api::PlaybackContext& ctx);

    /// User clicked a queue row to play it. Sets it as active and
    /// resolves a fresh URL.
    void playAt(int index);

    /// Queue-aware transport navigation shared by the player chrome
    /// and MPRIS.
    void playPreviousItem();
    void playNextItem();

    /// Drop the row at `index`. If it's the active row, in step 2
    /// we just clear `activeIndex`; auto-advance lands in step 4.
    void removeAt(int index);

    /// Reorder. `from` and `to` are 0-based; `to` is the destination
    /// index in the post-move list.
    void moveTo(int from, int to);

    /// Drop everything. Stops in-flight resolution.
    void clearAll();

    /// Retry an item previously marked `Failed`. Resets it to
    /// `Pending` and \u2014 if it's the active item or there is no
    /// active item \u2014 dispatches it.
    void retryFailed(int index);

    /// Hook for the embedded player's natural / error end-of-file
    /// event. Wired (libmpv-only) from `PlaybackController::endOfFile`.
    /// Behaviour:
    ///  - if a user-close was just signalled, that takes precedence:
    ///    revert the active row to `Pending` and stop;
    ///  - on a clean end-of-file (`reason` empty or "eof"), the
    ///    active row was watched to completion - remove it and
    ///    advance to the new slot 0;
    ///  - on `reason == "error"` or anything else, mark the row
    ///    `Failed` and skip-and-advance.
    /// When the queue is empty after this transition, emits
    /// `windowCloseRequested` so the host can hide the embedded
    /// player.
    void onPlayerEndOfFile(const QString& reason,
        const api::PlaybackContext& ctx);

    /// Hook for the embedded player's `userClosedWindow` signal.
    /// Reverts the active row to `Pending` and consumes the next
    /// `endOfFile("stop")` so user-close never auto-advances.
    void onPlayerUserClosed();

Q_SIGNALS:
    /// Asks the host (`MainController`) to hide the embedded player
    /// window because the queue is empty. Emitted on the natural
    /// end-of-file of the last item.
    void windowCloseRequested();

    /// Emitted right before / after a wholesale list change
    /// (`clearAll`, initial load). Fine-grained mutations use the
    /// row signals instead.
    void itemsAboutToReset();
    void itemsReset();

    /// Granular mutations \u2014 the QML model translates the
    /// about-to/finished pairs 1:1 to `beginInsertRows` /
    /// `endInsertRows`, `beginRemoveRows` / `endRemoveRows`, etc.
    void itemAboutToBeInserted(int index);
    void itemInserted(int index);
    void itemAboutToBeRemoved(int index);
    void itemRemoved(int index);
    void itemAboutToBeMoved(int from, int to);
    void itemMoved(int from, int to);
    void itemChanged(int index);

    /// Fired when `activeIndex()` changes (including \u2192 `-1`).
    void activeIndexChanged(int index);

    /// Status-bar / passive-notification fan-out. Wired into
    /// `MainController::passiveMessage` like the other controllers.
    void statusMessage(const QString& text, int timeoutMs = 3000);

private:
    /// Resolve a fresh stream URL for the active item and dispatch
    /// playback through `StreamActions`. Marks the item `Failed`
    /// and emits `itemChanged` if resolution fails (with no auto-
    /// advance \u2014 that lands in step 4).
    QCoro::Task<void> startActiveItem();

    /// Build a QueueItem from a Stream + display context. Captures
    /// the directUrl in `cachedDirectUrl` (in-memory only).
    static api::QueueItem buildItem(const api::Stream& s,
        const api::PlaybackContext& ctx);

    /// Synthesise a Stream that matches the QueueItem's release
    /// info, with `directUrl` set to the freshly-resolved URL. Used
    /// to hand off through `StreamActions::play`.
    static api::Stream toStream(const api::QueueItem& item,
        const QUrl& url);

    /// Build a PlaybackContext from the queue item (display fields
    /// only \u2014 streamRef and resumeSeconds are filled by
    /// `StreamActions::play`).
    static api::PlaybackContext toContext(const api::QueueItem& item);

    /// Resolve the index of the existing row for `key`, or -1.
    int indexOfKey(const api::PlaybackKey& key) const;

    /// Overwrite the row at `index` with fresh stream/context data
    /// for the same media key.
    void replaceRowFromStream(int index, const api::Stream& s,
        const api::PlaybackContext& ctx);

    /// Remove a row so the caller can reinsert it elsewhere without
    /// triggering active-playback side effects.
    void removeRowForReinsert(int index);

    /// Insert `item` at `atIndex`, persist, and emit itemInserted.
    /// Returns the (possibly clamped) insert position.
    int insertAt(api::QueueItem item, int atIndex);

    /// Persist via the store. Renumbers `order` to match list order.
    void persist();

    /// Update `m_activeIndex` and emit `activeIndexChanged` if
    /// changed. No-op when value is identical.
    void setActiveIndex(int index);

    /// Bump the resolve epoch, clear stale user-close state, and
    /// dispatch the current active row.
    void dispatchActiveItem();

    core::PlayQueueStore& m_store;
    api::TorrentioClient& m_torrentio;
    services::StreamActions& m_actions;
    const config::AppSettings& m_settings;
    const QString& m_rdToken;

    QList<api::QueueItem> m_items;
    int m_activeIndex = -1;

    /// Bumped on every dispatch / clear so a late co-await result
    /// from a stale resolve task is discarded.
    quint64 m_resolveEpoch = 0;

    /// Set true on `userClosedWindow`, cleared on the next
    /// `endOfFile`. Lets us treat the inevitable
    /// `endOfFile("stop")` that mpv emits during user-close as a
    /// no-op pause rather than an auto-advance trigger.
    bool m_userClosePending = false;
};

} // namespace kinema::controllers
