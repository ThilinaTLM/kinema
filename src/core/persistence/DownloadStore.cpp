// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/persistence/DownloadStore.h"

#include "core/persistence/Database.h"
#include "core/persistence/SqlUtil.h"
#include "kinema_log_download.h"

#include <QDateTime>
#include <QSqlError>
#include <QSqlQuery>
#include <QTimer>
#include <QVariant>

namespace kinema::core {

namespace {

using sql::isoUtc;
using sql::nullSafe;
using sql::parseIsoUtc;

constexpr const char* kSelectColumns =
    "asset_id, backend_kind, state, mode, cache_disposition, "
    "playback_key, media_kind, imdb_id, season, episode, "
    "title, series_title, episode_title, poster_url, "
    "info_hash, release_name, file_index, file_name_hint, "
    "quality_label, resolution, provider, "
    "expected_size_bytes, cached_size_bytes, last_error, complete, "
    "local_dir, added_at, updated_at, last_used_at";

domain::DownloadItem hydrate(const QSqlQuery& q)
{
    domain::DownloadItem it;
    it.assetId = q.value(0).toString();
    it.backendKind = static_cast<domain::DownloadBackendKind>(q.value(1).toInt());
    it.state = static_cast<domain::DownloadState>(q.value(2).toInt());
    it.mode = static_cast<domain::DownloadMode>(q.value(3).toInt());
    it.disposition = static_cast<domain::CacheDisposition>(q.value(4).toInt());

    // playback_key column at index 5 is denormalized for indexing;
    // the structured key is rebuilt from kind/imdb/season/episode.
    it.key.kind = static_cast<domain::MediaKind>(q.value(6).toInt());
    it.key.imdbId = q.value(7).toString();
    if (!q.value(8).isNull()) {
        it.key.season = q.value(8).toInt();
    }
    if (!q.value(9).isNull()) {
        it.key.episode = q.value(9).toInt();
    }
    it.title = q.value(10).toString();
    it.seriesTitle = q.value(11).toString();
    it.episodeTitle = q.value(12).toString();
    const auto poster = q.value(13).toString();
    if (!poster.isEmpty()) {
        it.poster = QUrl(poster);
    }
    it.infoHash = q.value(14).toString();
    it.releaseName = q.value(15).toString();
    it.fileIndex = q.value(16).toInt();
    it.fileNameHint = q.value(17).toString();
    it.qualityLabel = q.value(18).toString();
    it.resolution = q.value(19).toString();
    it.provider = q.value(20).toString();
    if (!q.value(21).isNull()) {
        it.expectedSizeBytes = q.value(21).toLongLong();
    }
    it.cachedSizeBytes = q.value(22).toLongLong();
    it.lastError = q.value(23).toString();
    it.complete = q.value(24).toInt() != 0;
    it.localDir = q.value(25).toString();
    it.addedAt = parseIsoUtc(q.value(26).toString());
    it.updatedAt = parseIsoUtc(q.value(27).toString());
    it.lastUsedAt = parseIsoUtc(q.value(28).toString());
    return it;
}

} // namespace

DownloadStore::DownloadStore(Database& db, QObject* parent)
    : QObject(parent)
    , m_db(db)
{
}

DownloadStore::~DownloadStore() = default;

void DownloadStore::scheduleChanged()
{
    if (m_changePending) {
        return;
    }
    m_changePending = true;
    QTimer::singleShot(0, this, [this] {
        m_changePending = false;
        Q_EMIT changed();
    });
}

QList<domain::DownloadItem> DownloadStore::loadAll() const
{
    QList<domain::DownloadItem> out;
    if (!m_db.isOpen()) {
        return out;
    }
    auto q = m_db.query();
    if (!q.exec(QStringLiteral(
            "SELECT %1 FROM download_items "
            "ORDER BY updated_at DESC")
                    .arg(QString::fromLatin1(kSelectColumns)))) {
        qCWarning(KINEMA_DOWNLOAD) << "DownloadStore::loadAll failed:"
                          << q.lastError().text();
        return out;
    }
    while (q.next()) {
        out.append(hydrate(q));
    }
    return out;
}

std::optional<domain::DownloadItem> DownloadStore::find(
    const QString& assetId) const
{
    if (!m_db.isOpen() || assetId.isEmpty()) {
        return std::nullopt;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral("SELECT %1 FROM download_items WHERE asset_id = ?")
                  .arg(QString::fromLatin1(kSelectColumns)));
    q.addBindValue(assetId);
    if (!q.exec() || !q.next()) {
        return std::nullopt;
    }
    return hydrate(q);
}

std::optional<domain::DownloadItem> DownloadStore::findForKey(
    const domain::PlaybackKey& key) const
{
    if (!m_db.isOpen() || !key.isValid()) {
        return std::nullopt;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral(
        "SELECT %1 FROM download_items WHERE playback_key = ? "
        "ORDER BY updated_at DESC LIMIT 1")
                  .arg(QString::fromLatin1(kSelectColumns)));
    q.addBindValue(key.storageKey());
    if (!q.exec() || !q.next()) {
        return std::nullopt;
    }
    return hydrate(q);
}

void DownloadStore::upsert(const domain::DownloadItem& item)
{
    if (!m_db.isOpen() || item.assetId.isEmpty()) {
        return;
    }
    const auto now = isoUtc(QDateTime::currentDateTimeUtc());

    auto q = m_db.query();
    q.prepare(QStringLiteral(R"(
        INSERT INTO download_items (
            asset_id, backend_kind, state, mode, cache_disposition,
            playback_key, media_kind, imdb_id, season, episode,
            title, series_title, episode_title, poster_url,
            info_hash, release_name, file_index, file_name_hint,
            quality_label, resolution, provider,
            expected_size_bytes, cached_size_bytes, last_error, complete,
            local_dir, added_at, updated_at, last_used_at
        ) VALUES (
            ?, ?, ?, ?, ?,
            ?, ?, ?, ?, ?,
            ?, ?, ?, ?,
            ?, ?, ?, ?,
            ?, ?, ?,
            ?, ?, ?, ?,
            ?, ?, ?, ?
        )
        ON CONFLICT(asset_id) DO UPDATE SET
            backend_kind        = excluded.backend_kind,
            state               = excluded.state,
            mode                = excluded.mode,
            cache_disposition   = excluded.cache_disposition,
            playback_key        = excluded.playback_key,
            media_kind          = excluded.media_kind,
            imdb_id             = excluded.imdb_id,
            season              = excluded.season,
            episode             = excluded.episode,
            title               = excluded.title,
            series_title        = excluded.series_title,
            episode_title       = excluded.episode_title,
            poster_url          = excluded.poster_url,
            info_hash           = excluded.info_hash,
            release_name        = excluded.release_name,
            file_index          = excluded.file_index,
            file_name_hint      = excluded.file_name_hint,
            quality_label       = excluded.quality_label,
            resolution          = excluded.resolution,
            provider            = excluded.provider,
            expected_size_bytes = excluded.expected_size_bytes,
            cached_size_bytes   = excluded.cached_size_bytes,
            last_error          = excluded.last_error,
            complete            = excluded.complete,
            local_dir           = excluded.local_dir,
            updated_at          = excluded.updated_at,
            last_used_at        = excluded.last_used_at
    )"));

    q.addBindValue(item.assetId);
    q.addBindValue(static_cast<int>(item.backendKind));
    q.addBindValue(static_cast<int>(item.state));
    q.addBindValue(static_cast<int>(item.mode));
    q.addBindValue(static_cast<int>(item.disposition));

    q.addBindValue(item.key.storageKey());
    q.addBindValue(static_cast<int>(item.key.kind));
    q.addBindValue(nullSafe(item.key.imdbId));
    q.addBindValue(item.key.season.has_value()
            ? QVariant(*item.key.season)
            : QVariant(QMetaType(QMetaType::Int)));
    q.addBindValue(item.key.episode.has_value()
            ? QVariant(*item.key.episode)
            : QVariant(QMetaType(QMetaType::Int)));

    q.addBindValue(nullSafe(item.title));
    q.addBindValue(nullSafe(item.seriesTitle));
    q.addBindValue(nullSafe(item.episodeTitle));
    // The schema is `poster_url TEXT NOT NULL DEFAULT ''`, so we
    // must send an actual empty string rather than a default-
    // constructed (null) QString — the latter binds as SQL NULL.
    q.addBindValue(item.poster.isEmpty()
            ? QStringLiteral("")
            : item.poster.toString());

    q.addBindValue(nullSafe(item.infoHash));
    q.addBindValue(nullSafe(item.releaseName));
    q.addBindValue(item.fileIndex);
    q.addBindValue(nullSafe(item.fileNameHint));

    q.addBindValue(nullSafe(item.qualityLabel));
    q.addBindValue(nullSafe(item.resolution));
    q.addBindValue(nullSafe(item.provider));

    q.addBindValue(item.expectedSizeBytes.has_value()
            ? QVariant(*item.expectedSizeBytes)
            : QVariant(QMetaType(QMetaType::LongLong)));
    q.addBindValue(item.cachedSizeBytes);
    q.addBindValue(nullSafe(item.lastError));
    q.addBindValue(item.complete ? 1 : 0);

    q.addBindValue(nullSafe(item.localDir));
    const auto added = item.addedAt.isValid() ? isoUtc(item.addedAt) : now;
    q.addBindValue(added);
    q.addBindValue(now);
    const auto used = item.lastUsedAt.isValid() ? isoUtc(item.lastUsedAt) : now;
    q.addBindValue(used);

    if (!q.exec()) {
        qCWarning(KINEMA_DOWNLOAD) << "DownloadStore::upsert failed:"
                          << q.lastError().text();
        return;
    }
    scheduleChanged();
}

void DownloadStore::updateProgress(const QString& assetId,
    qint64 cachedSizeBytes, bool complete)
{
    if (!m_db.isOpen() || assetId.isEmpty()) {
        return;
    }
    const auto now = isoUtc(QDateTime::currentDateTimeUtc());
    auto q = m_db.query();
    q.prepare(QStringLiteral(
        "UPDATE download_items SET "
        "cached_size_bytes = ?, complete = ?, "
        "updated_at = ?, last_used_at = ? "
        "WHERE asset_id = ?"));
    q.addBindValue(cachedSizeBytes);
    q.addBindValue(complete ? 1 : 0);
    q.addBindValue(now);
    q.addBindValue(now);
    q.addBindValue(assetId);
    if (!q.exec()) {
        qCWarning(KINEMA_DOWNLOAD) << "DownloadStore::updateProgress failed:"
                          << q.lastError().text();
        return;
    }
    scheduleChanged();
}

void DownloadStore::updateState(const QString& assetId,
    domain::DownloadState state, const QString& lastError)
{
    if (!m_db.isOpen() || assetId.isEmpty()) {
        return;
    }
    const auto now = isoUtc(QDateTime::currentDateTimeUtc());
    auto q = m_db.query();
    q.prepare(QStringLiteral(
        "UPDATE download_items SET "
        "state = ?, last_error = ?, updated_at = ? "
        "WHERE asset_id = ?"));
    q.addBindValue(static_cast<int>(state));
    q.addBindValue(nullSafe(lastError));
    q.addBindValue(now);
    q.addBindValue(assetId);
    if (!q.exec()) {
        qCWarning(KINEMA_DOWNLOAD) << "DownloadStore::updateState failed:"
                          << q.lastError().text();
        return;
    }
    scheduleChanged();
}

void DownloadStore::updateMode(const QString& assetId,
    domain::DownloadMode mode)
{
    if (!m_db.isOpen() || assetId.isEmpty()) {
        return;
    }
    const auto now = isoUtc(QDateTime::currentDateTimeUtc());
    auto q = m_db.query();
    q.prepare(QStringLiteral(
        "UPDATE download_items SET "
        "mode = ?, updated_at = ? "
        "WHERE asset_id = ?"));
    q.addBindValue(static_cast<int>(mode));
    q.addBindValue(now);
    q.addBindValue(assetId);
    if (!q.exec()) {
        qCWarning(KINEMA_DOWNLOAD) << "DownloadStore::updateMode failed:"
                          << q.lastError().text();
        return;
    }
    scheduleChanged();
}

DownloadStore::StartArgs DownloadStore::synthesiseStartArgs(
    const domain::DownloadItem& row)
{
    StartArgs out;

    out.ref.key = row.key;
    out.ref.infoHash = row.infoHash;
    out.ref.releaseName = row.releaseName;
    out.ref.fileIndex = row.fileIndex;
    out.ref.fileNameHint = row.fileNameHint;
    out.ref.qualityLabel = row.qualityLabel;
    out.ref.resolution = row.resolution;
    out.ref.provider = row.provider;
    out.ref.sizeBytes = row.expectedSizeBytes;

    out.stream.infoHash = row.infoHash;
    out.stream.releaseName = row.releaseName;
    out.stream.qualityLabel = row.qualityLabel;
    out.stream.resolution = row.resolution;
    out.stream.provider = row.provider;
    out.stream.fileIndex = row.fileIndex;
    out.stream.fileNameHint = row.fileNameHint;
    if (row.expectedSizeBytes) {
        out.stream.sizeBytes = *row.expectedSizeBytes;
    }

    out.ctx.key = row.key;
    out.ctx.title = row.title;
    out.ctx.seriesTitle = row.seriesTitle;
    out.ctx.episodeTitle = row.episodeTitle;
    out.ctx.poster = row.poster;

    return out;
}

void DownloadStore::setDisposition(const QString& assetId,
    domain::CacheDisposition d)
{
    if (!m_db.isOpen() || assetId.isEmpty()) {
        return;
    }
    const auto now = isoUtc(QDateTime::currentDateTimeUtc());
    auto q = m_db.query();
    q.prepare(QStringLiteral(
        "UPDATE download_items SET "
        "cache_disposition = ?, updated_at = ? "
        "WHERE asset_id = ?"));
    q.addBindValue(static_cast<int>(d));
    q.addBindValue(now);
    q.addBindValue(assetId);
    if (!q.exec()) {
        qCWarning(KINEMA_DOWNLOAD) << "DownloadStore::setDisposition failed:"
                          << q.lastError().text();
        return;
    }
    scheduleChanged();
}

void DownloadStore::touch(const QString& assetId)
{
    if (!m_db.isOpen() || assetId.isEmpty()) {
        return;
    }
    const auto now = isoUtc(QDateTime::currentDateTimeUtc());
    auto q = m_db.query();
    q.prepare(QStringLiteral(
        "UPDATE download_items SET last_used_at = ? WHERE asset_id = ?"));
    q.addBindValue(now);
    q.addBindValue(assetId);
    if (!q.exec()) {
        qCWarning(KINEMA_DOWNLOAD) << "DownloadStore::touch failed:"
                          << q.lastError().text();
    }
}

void DownloadStore::remove(const QString& assetId)
{
    if (!m_db.isOpen() || assetId.isEmpty()) {
        return;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral("DELETE FROM download_items WHERE asset_id = ?"));
    q.addBindValue(assetId);
    if (!q.exec()) {
        qCWarning(KINEMA_DOWNLOAD) << "DownloadStore::remove failed:"
                          << q.lastError().text();
        return;
    }
    scheduleChanged();
}

void DownloadStore::clear()
{
    if (!m_db.isOpen()) {
        return;
    }
    auto q = m_db.query();
    if (!q.exec(QStringLiteral("DELETE FROM download_items"))) {
        qCWarning(KINEMA_DOWNLOAD) << "DownloadStore::clear failed:"
                          << q.lastError().text();
        return;
    }
    scheduleChanged();
}

} // namespace kinema::core
