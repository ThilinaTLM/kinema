// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/PlaybackContext.h"

#include <QList>
#include <QObject>
#include <QString>

#include <QCoro/QCoroTask>

#include <optional>

namespace kinema::api {
class TorrentioClient;
}

namespace kinema::config {
class AppSettings;
}

namespace kinema::core {
class HistoryStore;
}

namespace kinema::services {
class StreamActions;
}

namespace kinema::ui::player {
class PlayerWindow;
}

namespace kinema::controllers {

/**
 * Mediator between the embedded player, the persistent
 * `core::HistoryStore`, and the play-time pipeline.
 *
 * Responsibilities:
 *   - Observe the embedded player's `time-pos` / `duration` / load /
 *     EOF signals and persist progress to the store.
 *   - Provide `resumeSecondsFor(key)` that StreamActions consults
 *     before issuing play. The in-memory position of the active
 *     session wins over the on-disk value so a mid-session stream
 *     swap resumes at the fresh timestamp.
 *   - Record every play-start (embedded or external) via
 *     `onPlayStarting(ctx)`, capturing the `HistoryStreamRef` so a
 *     later "Continue Watching" click can replay the same release.
 *   - Orchestrate one-click resume: fetch Torrentio, match by
 *     infoHash, and hand off to StreamActions. On a miss, signal the
 *     caller to fall back to the detail pane.
 *
 * Two-phase init: `PlayerWindow` and `StreamActions` are wired in
 * after construction because they form a small cycle with this
 * controller. Safe to leave either unset for headless tests; the
 * controller degrades gracefully.
 */
class HistoryController : public QObject
{
    Q_OBJECT
public:
    HistoryController(core::HistoryStore& store,
        api::TorrentioClient* torrentio,
        const config::AppSettings& settings,
        const QString& rdTokenRef,
        QObject* parent = nullptr);

    ~HistoryController() override;

    /// Wire the embedded player's signals. May be called with null
    /// (e.g. non-libmpv builds); disconnects first when re-wired.
    void setPlayerWindow(ui::player::PlayerWindow* window);

    /// Provide the StreamActions used by `resumeFromHistory`. Must be
    /// set before Continue-Watching cards can resume.
    void setStreamActions(services::StreamActions* actions);

    /// Called by StreamActions::play just before the launcher fires.
    /// Pins the context so later position updates can be attributed
    /// to it and persists the "last-played" row so external plays
    /// show up in Continue Watching too.
    void onPlayStarting(const api::PlaybackContext& ctx);

    /// Resume-time lookup. Returns the live in-memory position when
    /// the active session matches `key` (mid-session stream swap);
    /// otherwise the stored position.
    std::optional<qint64> resumeSecondsFor(const api::PlaybackKey& key) const;

    std::optional<api::HistoryEntry> find(const api::PlaybackKey& key) const;
    std::optional<api::HistoryEntry> findLatestForMedia(
        api::MediaKind kind, const QString& imdbId) const;

    QList<api::HistoryEntry> continueWatching(int maxItems = 30) const;

public Q_SLOTS:
    /// One-click resume. Re-fetches Torrentio for the entry's key,
    /// matches by `lastStream.infoHash`, and on a hit calls
    /// StreamActions::play with the stored resume position. On a
    /// miss emits `resumeFallbackRequested()` so MainWindow can open
    /// the detail pane instead.
    void resumeFromHistory(const api::HistoryEntry& entry);

Q_SIGNALS:
    /// Forwarded from HistoryStore::changed. Drives UI refreshes.
    void changed();

    /// Short status-bar messages ("Resuming\u2026", "Saved release no
    /// longer available \u2014 showing alternatives.").
    void statusMessage(const QString& text, int timeoutMs = 3000);

    /// The stored stream was not found in Torrentio's current
    /// response (or RD could not give us a direct URL). MainWindow
    /// opens the detail pane via the existing openFromHistory path
    /// so the user can pick another release.
    void resumeFallbackRequested(const api::HistoryEntry& entry);

private Q_SLOTS:
    void onFileLoaded();
    void onEndOfFile(const QString& reason);
    void onPositionChanged(double seconds);
    void onDurationChanged(double seconds);
    void onPersistTick();

private:
    QCoro::Task<void> resumeTask(api::HistoryEntry entry);
    void persistActive(bool force);

    core::HistoryStore& m_store;
    api::TorrentioClient* m_torrentio;
    const config::AppSettings& m_settings;
    const QString& m_rdToken;
    ui::player::PlayerWindow* m_player {};
    services::StreamActions* m_actions {};

    /// Set by onPlayStarting, promoted to m_active on fileLoaded.
    std::optional<api::PlaybackContext> m_pending;
    /// Current embedded-player session, if any.
    std::optional<api::PlaybackContext> m_active;
    double m_lastPosition = 0.0;
    double m_duration = 0.0;
    double m_lastPersistedPosition = 0.0;

    quint64 m_resumeEpoch = 0;
};

} // namespace kinema::controllers
