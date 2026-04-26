// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/PlaybackContext.h"

#include <QDateTime>
#include <QList>
#include <QObject>
#include <QSet>
#include <QString>

#include <optional>

namespace kinema::core {

class Database;

/**
 * DAO over the v3 `subtitle_cache` table. Mirrors `HistoryStore`'s
 * shape: every write goes through a single transaction, reads return
 * value-typed POD, and a closed/unopened `Database` turns every call
 * into a no-op so the rest of the app keeps working when the SQLite
 * layer is broken.
 *
 * The store is the source of truth for "what is on disk". The actual
 * `.srt` / `.vtt` files live under `core::cache::subtitlesDir()`; the
 * controller is responsible for keeping the file system in sync (by
 * writing files only after a successful insert and by deleting files
 * when rows are removed).
 */
class SubtitleCacheStore : public QObject
{
    Q_OBJECT
public:
    struct Entry {
        QString fileId;            ///< OpenSubtitles file_id, stringified
        QString imdbId;            ///< "tt0133093"
        std::optional<int> season; ///< NULL for movies
        std::optional<int> episode;
        QString language;          ///< e.g. "eng" (ISO 639-2)
        QString languageName;      ///< "English"
        QString releaseName;
        QString fileName;          ///< original upstream file_name
        QString format;            ///< "srt" / "vtt" / "ass"
        bool hearingImpaired = false;
        bool foreignPartsOnly = false;
        QString localPath;
        qint64 sizeBytes = 0;
        QDateTime addedAt;         ///< UTC
        QDateTime lastUsedAt;      ///< UTC
    };

    explicit SubtitleCacheStore(Database& db, QObject* parent = nullptr);
    ~SubtitleCacheStore() override;

    SubtitleCacheStore(const SubtitleCacheStore&) = delete;
    SubtitleCacheStore& operator=(const SubtitleCacheStore&) = delete;

    std::optional<Entry> findByFileId(const QString& fileId) const;

    /// All cached entries for a given media key. When `languages` is
    /// non-empty, results are filtered by ISO 639-2 code.
    QList<Entry> findFor(const api::PlaybackKey& key,
        const QStringList& languages = {}) const;

    /// Just the file ids for `key`, for fast "is this row cached?"
    /// lookups in the picker delegate.
    QSet<QString> cachedFileIds(const api::PlaybackKey& key) const;

    /// All entries across all keys. Used by the reconcile pass.
    QList<Entry> all() const;

    /// Insert a new row. Caller has already written the file at
    /// `entry.localPath`. Returns false on error.
    bool insert(const Entry& e);

    /// Bump `last_used_at` to now. Cheap, used on every cache hit.
    void touch(const QString& fileId);

    /// Remove the row only — caller is responsible for unlinking
    /// `local_path` from disk.
    void remove(const QString& fileId);

    qint64 totalSizeBytes() const;

    /// Oldest-first list of rows whose cumulative `size_bytes` covers
    /// at least `needToFree` bytes. Caller deletes in order, then
    /// removes rows.
    QList<Entry> evictionCandidates(qint64 needToFree) const;

    /// Drop every row. Caller unlinks the on-disk directory.
    void clearAll();

    /// Build the absolute on-disk path for a fresh download. Pure
    /// function; does NOT create directories.
    ///
    /// Sanitisation applied to `upstreamFileName`:
    ///   - strip any trailing extension,
    ///   - replace `< > : " / \\ | ? * \\0` with `_`,
    ///   - collapse whitespace runs to a single `_`,
    ///   - NFC-normalise,
    ///   - truncate to 120 UTF-8 bytes on a code-point boundary,
    ///   - append `-<currentMSecsSinceEpoch>.<format>`.
    /// `format` falls back to `srt` when empty / unknown.
    static QString buildLocalPath(const QString& imdbId,
        const QString& upstreamFileName,
        const QString& format);

Q_SIGNALS:
    /// Coalesced once per event-loop tick.
    void changed();

private:
    void scheduleChanged();

    Database& m_db;
    bool m_changePending = false;
};

} // namespace kinema::core
