// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/PlaybackContext.h"
#include "api/Subtitle.h"

#include <QList>
#include <QObject>
#include <QSet>
#include <QString>
#include <QStringList>

#include <QCoro/QCoroTask>

namespace kinema::api {
class OpenSubtitlesClient;
}

namespace kinema::config {
class CacheSettings;
class SubtitleSettings;
}

namespace kinema::core {
class SubtitleCacheStore;
}

namespace kinema::controllers {

/**
 * Owns the subtitle-search + download flow.
 *
 *   - `runQuery` issues a search against OpenSubtitles, with stale
 *     responses dropped via an epoch counter (canonical
 *     `SearchController::runQuery` shape).
 *   - `download` resolves a `fileId` against the cache first; on miss
 *     it requests a signed link, fetches the bytes, writes to disk,
 *     persists a row, runs an opportunistic LRU pass, and emits
 *     `downloadFinished`. `MainWindow` routes that signal to the
 *     QtWidgets `SubtitlesDialog` (which forwards to the player's
 *     `attachExternalSubtitle` for sideloading). The controller
 *     never includes any `ui/player/...` header.
 *   - `reconcileCacheOnStartup` walks the cache directory + table to
 *     drop orphan rows / orphan files and run an LRU pass if the
 *     budget is exceeded.
 *   - `setMoviehash` lets `PlaybackController` push a best-effort
 *     hash for the active stream so the next search can flag
 *     `moviehash_match` rows.
 *
 * `downloadEnabled()` is the gate: true iff the OpenSubtitles client
 * has all three credentials (api key + username + password). The
 * actual login is lazy — `OpenSubtitlesClient::ensureLoggedIn()`
 * runs on the first `search()` / `requestDownload()` and surfaces a
 * normal error if the password is wrong.
 */
class SubtitleController : public QObject
{
    Q_OBJECT
public:
    SubtitleController(api::OpenSubtitlesClient* client,
        core::SubtitleCacheStore* cache,
        const config::SubtitleSettings& settings,
        const config::CacheSettings& cacheSettings,
        QObject* parent = nullptr);

    bool isSearching() const noexcept { return m_searching; }
    QString lastError() const { return m_lastError; }
    bool downloadEnabled() const;
    QString moviehash() const { return m_moviehash; }

    /// The current search hits. Read by QML / the search model after
    /// `hitsChanged` fires.
    QList<api::SubtitleHit> hits() const { return m_hits; }

    /// Set of cached file ids for the currently queried key. Used by
    /// the search model to badge rows.
    QSet<QString> cachedFileIds() const { return m_cachedFileIds; }

    /// Currently active (sideloaded) `local_path`s — i.e. paths of
    /// downloaded subs that mpv has loaded, of which one is selected.
    /// The search model uses this to show a ✓ badge.
    QSet<QString> activeLocalPaths() const { return m_activeLocalPaths; }

    /// Last-seen `remaining` field on a `SubtitleDownloadTicket`. -1 if
    /// no successful download has happened this session. The dialog
    /// surfaces this in its status footer so users can see how close
    /// they are to OpenSubtitles' daily cap.
    int dailyDownloadsRemaining() const noexcept
    {
        return m_dailyDownloadsRemaining;
    }

public Q_SLOTS:
    /// Top-level entry from QML. Forwards every filter the picker
    /// surfaced. Empty `languages` means "no language filter".
    void runQuery(api::PlaybackKey key,
        QStringList languages,
        QString hearingImpaired,
        QString foreignPartsOnly,
        QString releaseFilter);

    /// Resolve `fileId` from cache or fetch + cache it.
    void download(QString fileId, api::PlaybackKey key);

    /// PlaybackController pushes a best-effort moviehash for the
    /// active stream. `setMoviehash("")` invalidates.
    void setMoviehash(QString hex);

    /// Convenience for stream-changed events.
    void clearMoviehash() { setMoviehash(QString {}); }

    /// Run on first event-loop tick after `Database::open()`. Drops
    /// orphan rows / files, runs an LRU pass if over budget. Cheap;
    /// safe to call from the main thread.
    void reconcileCacheOnStartup();

    /// Clear every cached subtitle (rows + files). Used by the
    /// settings page's "Clear subtitle cache" button.
    void clearAllCachedSubtitles();

    /// Tell the controller which `local_path`s are currently
    /// loaded into mpv. Driven by SubtitleTracksModel changes.
    void setActiveSubtitlePaths(const QStringList& paths);

    /// Re-emit the `downloadEnabledChanged` state — called after the
    /// settings page completes a successful "Test connection".
    void notifyAuthChanged();

Q_SIGNALS:
    void searchingChanged();
    void errorChanged();
    void downloadEnabledChanged(bool);
    void moviehashChanged(QString);
    void hitsChanged();
    void cacheChanged();
    void activeSubtitleChanged();

    /// Fired when a download succeeds. The QML layer attaches the
    /// file via `PlayerViewModel::attachExternalSubtitle`.
    void downloadFinished(QString fileId,
        QString localPath,
        QString language,
        QString languageName);
    void downloadFailed(QString fileId, QString reason);

    /// Fired when the daily quota counter is updated from a fresh
    /// `SubtitleDownloadTicket`. -1 means "unknown".
    void quotaChanged(int remaining);

    /// Status-bar / toast text. MainWindow forwards to the status bar.
    void statusMessage(const QString& text, int timeoutMs = 3000);

private:
    QCoro::Task<void> runSearchTask(api::SubtitleSearchQuery q);
    QCoro::Task<void> downloadTask(QString fileId, api::PlaybackKey key);

    void setSearching(bool);
    void setError(QString message);
    void evictIfOverBudget();
    void rebuildCachedFileIds(const api::PlaybackKey& key);

    api::OpenSubtitlesClient* m_client;
    core::SubtitleCacheStore* m_cache;
    const config::SubtitleSettings& m_settings;
    const config::CacheSettings& m_cacheSettings;

    QList<api::SubtitleHit> m_hits;
    QSet<QString> m_cachedFileIds;
    QSet<QString> m_activeLocalPaths;
    api::PlaybackKey m_currentKey;
    bool m_searching = false;
    QString m_lastError;
    QString m_moviehash;
    quint64 m_epoch = 0;
    int m_dailyDownloadsRemaining = -1;
};

} // namespace kinema::controllers
