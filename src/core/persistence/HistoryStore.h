// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/PlaybackContext.h"

#include <QList>
#include <QObject>

#include <optional>

namespace kinema::core {

class Database;

/**
 * DAO over the `history` table. The public API is deliberately shaped
 * as if backed by an in-memory store so consumers
 * (`controllers::HistoryController`, tests) remain oblivious to the
 * SQLite substrate.
 *
 * Semantics:
 *   - `record()` is an upsert on `key`. Progress-only ticks (empty
 *     `lastStream`) preserve the previously persisted stream ref so
 *     the resume pointer is never clobbered by a metadata-free
 *     update.
 *   - `finished` flips automatically when
 *     `position_sec / duration_sec >= finishedThreshold` (default
 *     0.9); this drives the Continue-Watching filter. The tick path
 *     stays conservative against accidental scrubs.
 *   - `recordSessionEnd()` is the "playback session ended" variant:
 *     it applies a per-kind, looser stop threshold (0.85 for movies,
 *     0.90 for episodes — see the auto-watched plan) and honours an
 *     optional credits-start hint derived from mpv chapter metadata.
 *     Natural EOF always finishes; `Error` never auto-finishes.
 *   - `changed()` is coalesced to once per event-loop tick.
 *   - Writes are synchronous. With WAL + synchronous=NORMAL each
 *     write is sub-millisecond; no debouncing needed.
 *   - A closed or unopened `Database` turns every mutating call into
 *     a no-op and every read into an empty result \u2014 the UI stays
 *     usable when the DB layer is broken.
 */
class HistoryStore : public QObject
{
    Q_OBJECT
public:
    /// How the playback session ended. Drives whether (and at what
    /// threshold) `recordSessionEnd()` auto-finishes the row.
    enum class SessionEndReason {
        NaturalEof, ///< mpv reached the literal end of file.
        UserStop,   ///< user closed the player or hit stop.
        Error,      ///< playback aborted; never auto-finish.
    };

    explicit HistoryStore(Database& db, QObject* parent = nullptr);
    ~HistoryStore() override;

    HistoryStore(const HistoryStore&) = delete;
    HistoryStore& operator=(const HistoryStore&) = delete;

    /// Drop `finished = 1 AND last_watched_at < now()-365 days` rows.
    /// Idempotent; safe to call once at startup.
    void runRetentionPass();

    /// Override the default 365-day retention window. Primarily for
    /// tests.
    void setRetentionDays(int days);

    /// Override the finished-at threshold (fraction of duration).
    /// Primarily for tests. Default 0.9.
    void setFinishedThreshold(double fraction);
    double finishedThreshold() const noexcept { return m_finishedThreshold; }

    /// Override the per-kind stop-threshold applied by
    /// `recordSessionEnd()` when `reason == UserStop`. Defaults are
    /// 0.85 for movies and 0.90 for episodes (see plan). Bounded to
    /// [0.5, 1.0].
    void setStopFinishedThreshold(domain::MediaKind kind, double fraction);
    double stopFinishedThreshold(domain::MediaKind kind) const noexcept;

    std::optional<domain::HistoryEntry> find(const domain::PlaybackKey& key) const;

    /// For movies: returns the movie entry if any. For series:
    /// returns the most-recent entry for that imdbId across all
    /// seasons/episodes. Finished flag is respected \u2014 a finished
    /// entry still resolves if it's the most recent.
    std::optional<domain::HistoryEntry> findLatestForMedia(
        domain::MediaKind kind, const QString& imdbId) const;

    /// Most-recent in-progress entries for the Continue-Watching row.
    /// Series are deduplicated to the latest-watched episode per
    /// imdbId. `finished` entries are excluded.
    QList<domain::HistoryEntry> continueWatching(int limit = 30) const;

    /// Upsert one entry. Empty `lastStream` on an existing row
    /// preserves the previously stored ref.
    void record(const domain::HistoryEntry& entry);

    /// Upsert one entry at the end of a playback session. Compared
    /// to `record()`:
    ///   - `NaturalEof`  → always finishes the row.
    ///   - `UserStop`   → finishes when either the chapter-derived
    ///                    credits hint is reached (`position >=
    ///                    creditsStartSec - 2.0`) or the per-kind
    ///                    stop threshold is met.
    ///   - `Error`      → forwards to `record()` (no force, no
    ///                    chapter hint).
    /// Never un-finishes an already-finished row.
    void recordSessionEnd(const domain::HistoryEntry& entry,
        SessionEndReason reason,
        std::optional<double> creditsStartSec = std::nullopt);

    void remove(const domain::PlaybackKey& key);
    void clear();

    /// Update only the remembered subtitle language for `key`.
    /// No-op when no row exists; emits `changed()` on success.
    /// Used by the subtitle download flow so the next visit pre-ticks
    /// the language filter on the picker.
    void setRememberedSubtitleLang(const domain::PlaybackKey& key,
        const QString& lang);

Q_SIGNALS:
    /// Fired once per event-loop tick regardless of how many
    /// record/remove calls happened. Drives UI refresh.
    void changed();

private:
    void scheduleChanged();
    /// Shared SQL upsert. Both `record()` and `recordSessionEnd()`
    /// land here after computing the `finished` flag.
    void upsert(const domain::HistoryEntry& entry);

    Database& m_db;
    double m_finishedThreshold = 0.9;
    double m_stopFinishedThresholdMovie = 0.85;
    double m_stopFinishedThresholdSeries = 0.90;
    int m_retentionDays = 365;
    bool m_changePending = false;
};

} // namespace kinema::core
