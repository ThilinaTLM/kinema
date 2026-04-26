// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/SubtitleCacheStore.h"

#include "core/CachePaths.h"
#include "core/Database.h"
#include "kinema_debug.h"

#include <QDateTime>
#include <QDir>
#include <QSqlDatabase>
#include <QSqlError>
#include <QSqlQuery>
#include <QTimeZone>
#include <QTimer>
#include <QVariant>

namespace kinema::core {

namespace {

constexpr const char* kIsoFmt = "yyyy-MM-ddTHH:mm:ssZ";

QString isoUtc(const QDateTime& dt)
{
    return dt.toUTC().toString(QString::fromLatin1(kIsoFmt));
}

QDateTime parseIsoUtc(const QString& s)
{
    auto dt = QDateTime::fromString(s, Qt::ISODate);
    if (!dt.isValid()) {
        dt = QDateTime::fromString(s, QString::fromLatin1(kIsoFmt));
    }
    if (dt.isValid()) {
        dt.setTimeZone(QTimeZone::utc());
    }
    return dt;
}

QString nullSafe(const QString& s)
{
    return s.isNull() ? QString::fromLatin1("") : s;
}

constexpr const char* kSelectColumns =
    "file_id, imdb_id, season, episode, "
    "language, language_name, release_name, file_name, format, "
    "hearing_impaired, foreign_parts_only, "
    "local_path, size_bytes, added_at, last_used_at";

SubtitleCacheStore::Entry hydrate(const QSqlQuery& q)
{
    SubtitleCacheStore::Entry e;
    e.fileId = q.value(0).toString();
    e.imdbId = q.value(1).toString();
    if (!q.value(2).isNull()) {
        e.season = q.value(2).toInt();
    }
    if (!q.value(3).isNull()) {
        e.episode = q.value(3).toInt();
    }
    e.language = q.value(4).toString();
    e.languageName = q.value(5).toString();
    e.releaseName = q.value(6).toString();
    e.fileName = q.value(7).toString();
    e.format = q.value(8).toString();
    e.hearingImpaired = q.value(9).toInt() != 0;
    e.foreignPartsOnly = q.value(10).toInt() != 0;
    e.localPath = q.value(11).toString();
    e.sizeBytes = q.value(12).toLongLong();
    e.addedAt = parseIsoUtc(q.value(13).toString());
    e.lastUsedAt = parseIsoUtc(q.value(14).toString());
    return e;
}

/// Replace every occurrence of any character in `bad` with '_'.
QString replaceChars(QString s, QStringView bad)
{
    for (auto ch : bad) {
        s.replace(ch, QLatin1Char('_'));
    }
    return s;
}

/// Collapse runs of whitespace (any Unicode whitespace) into a single
/// underscore. Trims leading/trailing underscores afterwards.
QString collapseWhitespace(const QString& s)
{
    QString out;
    out.reserve(s.size());
    bool inRun = false;
    for (auto ch : s) {
        if (ch.isSpace()) {
            if (!inRun) {
                out.append(QLatin1Char('_'));
                inRun = true;
            }
        } else {
            out.append(ch);
            inRun = false;
        }
    }
    while (out.startsWith(QLatin1Char('_'))) {
        out.remove(0, 1);
    }
    while (out.endsWith(QLatin1Char('_'))) {
        out.chop(1);
    }
    return out;
}

/// Truncate `s` so the UTF-8 encoding does not exceed `maxBytes`.
/// Truncates on a code-point boundary so the result is always valid
/// UTF-8 and a valid QString.
QString truncateUtf8Bytes(const QString& s, int maxBytes)
{
    const QByteArray utf8 = s.toUtf8();
    if (utf8.size() <= maxBytes) {
        return s;
    }
    int cut = maxBytes;
    // Walk back until we land on a UTF-8 lead byte. UTF-8 trailing
    // bytes match `10xxxxxx` (0x80..0xBF).
    while (cut > 0 && (static_cast<unsigned char>(utf8[cut]) & 0xC0) == 0x80) {
        --cut;
    }
    return QString::fromUtf8(utf8.constData(), cut);
}

QString sanitizeFormat(const QString& format)
{
    const auto f = format.trimmed().toLower();
    if (f == QLatin1String("vtt")
        || f == QLatin1String("ass")
        || f == QLatin1String("ssa")
        || f == QLatin1String("sub")
        || f == QLatin1String("srt")) {
        return f;
    }
    return QStringLiteral("srt");
}

} // namespace

SubtitleCacheStore::SubtitleCacheStore(Database& db, QObject* parent)
    : QObject(parent)
    , m_db(db)
{
}

SubtitleCacheStore::~SubtitleCacheStore() = default;

std::optional<SubtitleCacheStore::Entry> SubtitleCacheStore::findByFileId(
    const QString& fileId) const
{
    if (!m_db.isOpen() || fileId.isEmpty()) {
        return std::nullopt;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral("SELECT ") + QString::fromLatin1(kSelectColumns)
        + QStringLiteral(" FROM subtitle_cache WHERE file_id = ?"));
    q.addBindValue(fileId);
    if (!q.exec() || !q.next()) {
        return std::nullopt;
    }
    return hydrate(q);
}

QList<SubtitleCacheStore::Entry> SubtitleCacheStore::findFor(
    const api::PlaybackKey& key, const QStringList& languages) const
{
    QList<Entry> out;
    if (!m_db.isOpen() || !key.isValid()) {
        return out;
    }
    QString sql = QStringLiteral("SELECT ")
        + QString::fromLatin1(kSelectColumns)
        + QStringLiteral(" FROM subtitle_cache WHERE imdb_id = ?");
    if (key.season.has_value()) {
        sql += QStringLiteral(" AND season = ?");
    } else {
        sql += QStringLiteral(" AND season IS NULL");
    }
    if (key.episode.has_value()) {
        sql += QStringLiteral(" AND episode = ?");
    } else {
        sql += QStringLiteral(" AND episode IS NULL");
    }
    if (!languages.isEmpty()) {
        QStringList placeholders;
        placeholders.reserve(languages.size());
        for (int i = 0; i < languages.size(); ++i) {
            placeholders << QStringLiteral("?");
        }
        sql += QStringLiteral(" AND language IN (") + placeholders.join(QLatin1Char(','))
            + QStringLiteral(")");
    }
    sql += QStringLiteral(" ORDER BY last_used_at DESC");

    auto q = m_db.query();
    q.prepare(sql);
    q.addBindValue(key.imdbId);
    if (key.season.has_value()) {
        q.addBindValue(*key.season);
    }
    if (key.episode.has_value()) {
        q.addBindValue(*key.episode);
    }
    for (const auto& l : languages) {
        q.addBindValue(l);
    }
    if (!q.exec()) {
        qCWarning(KINEMA)
            << "SubtitleCacheStore: findFor failed:"
            << q.lastError().text();
        return out;
    }
    while (q.next()) {
        out.append(hydrate(q));
    }
    return out;
}

QSet<QString> SubtitleCacheStore::cachedFileIds(
    const api::PlaybackKey& key) const
{
    QSet<QString> out;
    if (!m_db.isOpen() || !key.isValid()) {
        return out;
    }
    QString sql = QStringLiteral(
        "SELECT file_id FROM subtitle_cache WHERE imdb_id = ?");
    if (key.season.has_value()) {
        sql += QStringLiteral(" AND season = ?");
    } else {
        sql += QStringLiteral(" AND season IS NULL");
    }
    if (key.episode.has_value()) {
        sql += QStringLiteral(" AND episode = ?");
    } else {
        sql += QStringLiteral(" AND episode IS NULL");
    }
    auto q = m_db.query();
    q.prepare(sql);
    q.addBindValue(key.imdbId);
    if (key.season.has_value()) {
        q.addBindValue(*key.season);
    }
    if (key.episode.has_value()) {
        q.addBindValue(*key.episode);
    }
    if (!q.exec()) {
        return out;
    }
    while (q.next()) {
        out.insert(q.value(0).toString());
    }
    return out;
}

QList<SubtitleCacheStore::Entry> SubtitleCacheStore::all() const
{
    QList<Entry> out;
    if (!m_db.isOpen()) {
        return out;
    }
    auto q = m_db.query();
    if (!q.exec(QStringLiteral("SELECT ")
            + QString::fromLatin1(kSelectColumns)
            + QStringLiteral(" FROM subtitle_cache"))) {
        return out;
    }
    while (q.next()) {
        out.append(hydrate(q));
    }
    return out;
}

bool SubtitleCacheStore::insert(const Entry& e)
{
    if (!m_db.isOpen() || e.fileId.isEmpty()) {
        return false;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral(R"(
        INSERT INTO subtitle_cache (
            file_id, imdb_id, season, episode,
            language, language_name, release_name, file_name, format,
            hearing_impaired, foreign_parts_only,
            local_path, size_bytes, added_at, last_used_at
        ) VALUES (?, ?, ?, ?,  ?, ?, ?, ?, ?,  ?, ?,  ?, ?, ?, ?)
        ON CONFLICT(file_id) DO UPDATE SET
            imdb_id = excluded.imdb_id,
            season = excluded.season,
            episode = excluded.episode,
            language = excluded.language,
            language_name = excluded.language_name,
            release_name = excluded.release_name,
            file_name = excluded.file_name,
            format = excluded.format,
            hearing_impaired = excluded.hearing_impaired,
            foreign_parts_only = excluded.foreign_parts_only,
            local_path = excluded.local_path,
            size_bytes = excluded.size_bytes,
            added_at = excluded.added_at,
            last_used_at = excluded.last_used_at
    )"));
    q.addBindValue(e.fileId);
    q.addBindValue(e.imdbId);
    q.addBindValue(e.season ? QVariant(*e.season) : QVariant());
    q.addBindValue(e.episode ? QVariant(*e.episode) : QVariant());
    q.addBindValue(nullSafe(e.language));
    q.addBindValue(nullSafe(e.languageName));
    q.addBindValue(nullSafe(e.releaseName));
    q.addBindValue(nullSafe(e.fileName));
    q.addBindValue(nullSafe(e.format));
    q.addBindValue(e.hearingImpaired ? 1 : 0);
    q.addBindValue(e.foreignPartsOnly ? 1 : 0);
    q.addBindValue(nullSafe(e.localPath));
    q.addBindValue(e.sizeBytes);
    q.addBindValue(isoUtc(e.addedAt.isValid()
            ? e.addedAt
            : QDateTime::currentDateTimeUtc()));
    q.addBindValue(isoUtc(e.lastUsedAt.isValid()
            ? e.lastUsedAt
            : QDateTime::currentDateTimeUtc()));

    if (!q.exec()) {
        qCWarning(KINEMA)
            << "SubtitleCacheStore: insert failed for" << e.fileId
            << "\u2014" << q.lastError().text();
        return false;
    }
    scheduleChanged();
    return true;
}

void SubtitleCacheStore::touch(const QString& fileId)
{
    if (!m_db.isOpen() || fileId.isEmpty()) {
        return;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral(
        "UPDATE subtitle_cache SET last_used_at = ? WHERE file_id = ?"));
    q.addBindValue(isoUtc(QDateTime::currentDateTimeUtc()));
    q.addBindValue(fileId);
    if (!q.exec()) {
        qCWarning(KINEMA)
            << "SubtitleCacheStore: touch failed:"
            << q.lastError().text();
        return;
    }
    if (q.numRowsAffected() > 0) {
        scheduleChanged();
    }
}

void SubtitleCacheStore::remove(const QString& fileId)
{
    if (!m_db.isOpen() || fileId.isEmpty()) {
        return;
    }
    auto q = m_db.query();
    q.prepare(QStringLiteral(
        "DELETE FROM subtitle_cache WHERE file_id = ?"));
    q.addBindValue(fileId);
    if (!q.exec()) {
        qCWarning(KINEMA)
            << "SubtitleCacheStore: remove failed:"
            << q.lastError().text();
        return;
    }
    if (q.numRowsAffected() > 0) {
        scheduleChanged();
    }
}

qint64 SubtitleCacheStore::totalSizeBytes() const
{
    if (!m_db.isOpen()) {
        return 0;
    }
    auto q = m_db.query();
    if (!q.exec(QStringLiteral(
            "SELECT COALESCE(SUM(size_bytes), 0) FROM subtitle_cache"))) {
        return 0;
    }
    if (!q.next()) {
        return 0;
    }
    return q.value(0).toLongLong();
}

QList<SubtitleCacheStore::Entry> SubtitleCacheStore::evictionCandidates(
    qint64 needToFree) const
{
    QList<Entry> out;
    if (!m_db.isOpen() || needToFree <= 0) {
        return out;
    }
    auto q = m_db.query();
    if (!q.exec(QStringLiteral("SELECT ")
            + QString::fromLatin1(kSelectColumns)
            + QStringLiteral(" FROM subtitle_cache "
                             "ORDER BY last_used_at ASC"))) {
        return out;
    }
    qint64 freed = 0;
    while (q.next()) {
        Entry e = hydrate(q);
        out.append(e);
        freed += e.sizeBytes;
        if (freed >= needToFree) {
            break;
        }
    }
    return out;
}

void SubtitleCacheStore::clearAll()
{
    if (!m_db.isOpen()) {
        return;
    }
    auto q = m_db.query();
    if (!q.exec(QStringLiteral("DELETE FROM subtitle_cache"))) {
        qCWarning(KINEMA)
            << "SubtitleCacheStore: clearAll failed:"
            << q.lastError().text();
        return;
    }
    scheduleChanged();
}

QString SubtitleCacheStore::buildLocalPath(const QString& imdbId,
    const QString& upstreamFileName, const QString& format)
{
    // Strip any trailing extension from the upstream name. We compare
    // case-insensitively against a few common subtitle suffixes, so a
    // file like "Movie.S01E03.eng.srt" becomes "Movie.S01E03.eng".
    QString base = upstreamFileName;
    if (base.isEmpty()) {
        base = QStringLiteral("subtitle");
    }
    static const QStringList kKnownExt = {
        QStringLiteral(".srt"), QStringLiteral(".vtt"),
        QStringLiteral(".ass"), QStringLiteral(".ssa"),
        QStringLiteral(".sub"), QStringLiteral(".txt"),
    };
    for (const auto& ext : kKnownExt) {
        if (base.size() > ext.size()
            && base.endsWith(ext, Qt::CaseInsensitive)) {
            base.chop(ext.size());
            break;
        }
    }

    // FS-illegal characters → '_'. Includes the NUL we never expect
    // but still defend against, plus the Windows reserved set so the
    // cache survives copies onto NTFS / FAT volumes.
    base = replaceChars(base,
        QStringLiteral("<>:\"/\\|?*\0"));

    // Collapse Unicode whitespace runs and trim underscores.
    base = collapseWhitespace(base);

    // NFC-normalise so a copy-paste from a macOS finder filename
    // (NFD) lands at the same path as a Linux-side encoding of the
    // same Unicode string.
    base = base.normalized(QString::NormalizationForm_C);

    if (base.isEmpty()) {
        base = QStringLiteral("subtitle");
    }

    base = truncateUtf8Bytes(base, 120);
    if (base.isEmpty()) {
        base = QStringLiteral("subtitle");
    }

    const QString ext = sanitizeFormat(format);
    const qint64 ts = QDateTime::currentMSecsSinceEpoch();

    const QString fileName = QStringLiteral("%1-%2.%3")
                                 .arg(base)
                                 .arg(ts)
                                 .arg(ext);

    QDir d = cache::subtitlesDir();
    return d.absoluteFilePath(imdbId + QLatin1Char('/') + fileName);
}

void SubtitleCacheStore::scheduleChanged()
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

} // namespace kinema::core
