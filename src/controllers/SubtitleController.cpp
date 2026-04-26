// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/SubtitleController.h"

#include "api/OpenSubtitlesClient.h"
#include "config/CacheSettings.h"
#include "config/SubtitleSettings.h"
#include "core/CachePaths.h"
#include "core/HttpError.h"
#include "core/HttpErrorPresenter.h"
#include "core/Language.h"
#include "core/SubtitleCacheStore.h"
#include "kinema_debug.h"

#include <KLocalizedString>

#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QFileInfo>
#include <QSaveFile>

#include <algorithm>

namespace kinema::controllers {

namespace {

bool matchesReleaseFilter(const api::SubtitleHit& hit, const QString& filter)
{
    if (filter.isEmpty()) {
        return true;
    }
    return hit.releaseName.contains(filter, Qt::CaseInsensitive)
        || hit.fileName.contains(filter, Qt::CaseInsensitive);
}

QString writeTempThenRename(const QString& finalPath, const QByteArray& bytes)
{
    QDir d = QFileInfo(finalPath).absoluteDir();
    if (!d.exists()) {
        d.mkpath(QStringLiteral("."));
    }

    // QSaveFile gives us atomic-rename semantics for free. The
    // ".part" extension is implicit.
    QSaveFile out(finalPath);
    if (!out.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        return QString {};
    }
    if (out.write(bytes) != bytes.size()) {
        out.cancelWriting();
        return QString {};
    }
    if (!out.commit()) {
        return QString {};
    }
    return finalPath;
}

} // namespace

SubtitleController::SubtitleController(api::OpenSubtitlesClient* client,
    core::SubtitleCacheStore* cache,
    const config::SubtitleSettings& settings,
    const config::CacheSettings& cacheSettings,
    QObject* parent)
    : QObject(parent)
    , m_client(client)
    , m_cache(cache)
    , m_settings(settings)
    , m_cacheSettings(cacheSettings)
{
}

bool SubtitleController::downloadEnabled() const
{
    // Gate on credentials only — login is lazy inside
    // OpenSubtitlesClient::search() / requestDownload(). Requiring a
    // cached JWT here would create a chicken-and-egg: the entry would
    // stay disabled until the first successful call, which the user
    // can't make because it's disabled.
    return m_client && m_client->hasCredentials();
}

void SubtitleController::setSearching(bool s)
{
    if (m_searching == s) {
        return;
    }
    m_searching = s;
    Q_EMIT searchingChanged();
}

void SubtitleController::setError(QString message)
{
    m_lastError = std::move(message);
    Q_EMIT errorChanged();
}

void SubtitleController::setMoviehash(QString hex)
{
    if (m_moviehash == hex) {
        return;
    }
    m_moviehash = std::move(hex);
    qCDebug(KINEMA) << "SubtitleController: moviehash ="
                    << (m_moviehash.isEmpty() ? "<unset>" : qPrintable(m_moviehash));
    Q_EMIT moviehashChanged(m_moviehash);
}

void SubtitleController::notifyAuthChanged()
{
    Q_EMIT downloadEnabledChanged(downloadEnabled());
}

void SubtitleController::setActiveSubtitlePaths(const QStringList& paths)
{
    QSet<QString> next;
    for (const auto& p : paths) {
        if (!p.isEmpty()) {
            next.insert(p);
        }
    }
    if (next == m_activeLocalPaths) {
        return;
    }
    m_activeLocalPaths = std::move(next);
    Q_EMIT activeSubtitleChanged();
}

void SubtitleController::runQuery(api::PlaybackKey key,
    QStringList languages,
    QString hearingImpaired,
    QString foreignPartsOnly,
    QString releaseFilter)
{
    if (!key.isValid()) {
        setError(i18n("Cannot search subtitles without an IMDb id."));
        return;
    }
    m_currentKey = key;
    api::SubtitleSearchQuery q;
    q.key = std::move(key);
    q.languages = std::move(languages);
    q.hearingImpaired = std::move(hearingImpaired);
    q.foreignPartsOnly = std::move(foreignPartsOnly);
    q.releaseFilter = std::move(releaseFilter);
    q.moviehash = m_moviehash;

    auto t = runSearchTask(std::move(q));
    Q_UNUSED(t);
}

QCoro::Task<void> SubtitleController::runSearchTask(api::SubtitleSearchQuery q)
{
    const auto myEpoch = ++m_epoch;
    setError({});
    setSearching(true);

    try {
        auto hits = co_await m_client->search(q);
        if (myEpoch != m_epoch) {
            co_return;
        }

        // Client-side release-name filter.
        if (!q.releaseFilter.trimmed().isEmpty()) {
            const auto needle = q.releaseFilter.trimmed();
            hits.erase(std::remove_if(hits.begin(), hits.end(),
                           [&needle](const api::SubtitleHit& h) {
                               return !matchesReleaseFilter(h, needle);
                           }),
                hits.end());
        }

        // moviehash_match first; ties broken by download_count desc.
        std::stable_sort(hits.begin(), hits.end(),
            [](const api::SubtitleHit& a, const api::SubtitleHit& b) {
                if (a.moviehashMatch != b.moviehashMatch) {
                    return a.moviehashMatch && !b.moviehashMatch;
                }
                return a.downloadCount > b.downloadCount;
            });

        m_hits = std::move(hits);
        rebuildCachedFileIds(q.key);
        Q_EMIT hitsChanged();

        Q_EMIT statusMessage(i18ncp("@info:status",
                                 "%1 subtitle found", "%1 subtitles found",
                                 m_hits.size()),
            3000);
    } catch (const std::exception& e) {
        if (myEpoch != m_epoch) {
            co_return;
        }
        setError(core::describeError(e, "subtitle search"));
        m_hits.clear();
        Q_EMIT hitsChanged();
        Q_EMIT statusMessage(m_lastError, 6000);
    }
    setSearching(false);
}

void SubtitleController::download(QString fileId, api::PlaybackKey key)
{
    auto t = downloadTask(std::move(fileId), std::move(key));
    Q_UNUSED(t);
}

QCoro::Task<void> SubtitleController::downloadTask(QString fileId,
    api::PlaybackKey key)
{
    if (fileId.isEmpty()) {
        co_return;
    }

    // Cache hit short-circuit.
    if (m_cache) {
        if (auto cached = m_cache->findByFileId(fileId);
            cached.has_value()
            && QFile::exists(cached->localPath)) {
            qCDebug(KINEMA)
                << "SubtitleController: cache hit for" << fileId;
            m_cache->touch(fileId);
            Q_EMIT downloadFinished(fileId, cached->localPath,
                cached->language, cached->languageName);
            Q_EMIT statusMessage(
                i18nc("@info:status", "Subtitle loaded from cache."), 3000);
            co_return;
        }
    }

    qCDebug(KINEMA) << "SubtitleController: cache miss for" << fileId
                    << "— fetching from OpenSubtitles";

    // Find the matching hit so we can fill display metadata.
    const api::SubtitleHit* hit = nullptr;
    for (const auto& h : m_hits) {
        if (h.fileId == fileId) {
            hit = &h;
            break;
        }
    }

    api::SubtitleDownloadTicket ticket;
    QByteArray bytes;
    try {
        ticket = co_await m_client->requestDownload(fileId);
        // Surface the quota counter even when the link came back
        // empty — the API still tells us how many we have left (or
        // 0 when exhausted).
        if (m_dailyDownloadsRemaining != ticket.remaining) {
            m_dailyDownloadsRemaining = ticket.remaining;
            Q_EMIT quotaChanged(m_dailyDownloadsRemaining);
        }
        if (ticket.link.isEmpty() || !ticket.link.isValid()) {
            // Quota-exhausted path.
            qCDebug(KINEMA)
                << "SubtitleController: quota exhausted, reset at"
                << ticket.resetAt.toString(Qt::ISODate);
            QString msg;
            if (ticket.resetAt.isValid()) {
                msg = i18nc("@info",
                    "Daily download quota exceeded. Try again at %1.",
                    ticket.resetAt.toLocalTime().toString(Qt::TextDate));
            } else {
                msg = i18nc("@info",
                    "Daily download quota exceeded.");
            }
            setError(msg);
            Q_EMIT downloadFailed(fileId, msg);
            Q_EMIT statusMessage(msg, 6000);
            co_return;
        }
        bytes = co_await m_client->fetchFileBytes(ticket.link);
    } catch (const std::exception& e) {
        const auto msg = core::describeError(e, "subtitle download");
        setError(msg);
        Q_EMIT downloadFailed(fileId, msg);
        Q_EMIT statusMessage(msg, 6000);
        co_return;
    }

    const QString fileName = !ticket.fileName.isEmpty()
        ? ticket.fileName
        : (hit ? hit->fileName : QStringLiteral("subtitle.srt"));
    QString format = ticket.format;
    if (format.isEmpty() && hit) {
        format = hit->format;
    }
    if (format.isEmpty()) {
        format = QStringLiteral("srt");
    }

    const auto localPath = core::SubtitleCacheStore::buildLocalPath(
        key.imdbId, fileName, format);
    const auto written = writeTempThenRename(localPath, bytes);
    if (written.isEmpty()) {
        const auto msg = i18nc("@info",
            "Could not write subtitle file to disk.");
        setError(msg);
        Q_EMIT downloadFailed(fileId, msg);
        Q_EMIT statusMessage(msg, 6000);
        co_return;
    }

    core::SubtitleCacheStore::Entry entry;
    entry.fileId = fileId;
    entry.imdbId = key.imdbId;
    entry.season = key.season;
    entry.episode = key.episode;
    entry.language = hit ? hit->language : QString {};
    entry.languageName = hit ? hit->languageName
                             : core::language::displayName(entry.language);
    entry.releaseName = hit ? hit->releaseName : QString {};
    entry.fileName = fileName;
    entry.format = format;
    entry.hearingImpaired = hit ? hit->hearingImpaired : false;
    entry.foreignPartsOnly = hit ? hit->foreignPartsOnly : false;
    entry.localPath = written;
    entry.sizeBytes = QFileInfo(written).size();
    entry.addedAt = QDateTime::currentDateTimeUtc();
    entry.lastUsedAt = entry.addedAt;

    if (m_cache && !m_cache->insert(entry)) {
        qCWarning(KINEMA)
            << "SubtitleController: cache row insert failed for" << fileId;
        // The file is on disk but unreferenced; remove it so the
        // reconcile pass doesn't have to clean up later.
        QFile::remove(written);
        const auto msg = i18nc("@info",
            "Could not record subtitle in cache index.");
        Q_EMIT downloadFailed(fileId, msg);
        Q_EMIT statusMessage(msg, 6000);
        co_return;
    }

    rebuildCachedFileIds(m_currentKey);
    Q_EMIT cacheChanged();

    evictIfOverBudget();

    Q_EMIT downloadFinished(fileId, written, entry.language, entry.languageName);
    Q_EMIT statusMessage(
        i18nc("@info:status", "Subtitle downloaded (%1).", entry.languageName),
        3000);
}

void SubtitleController::evictIfOverBudget()
{
    if (!m_cache) {
        return;
    }
    const qint64 budget = static_cast<qint64>(m_cacheSettings.subtitleBudgetMb())
        * 1024 * 1024;
    const qint64 total = m_cache->totalSizeBytes();
    if (total <= budget) {
        return;
    }
    const qint64 needed = total - budget;
    const auto victims = m_cache->evictionCandidates(needed);
    for (const auto& v : victims) {
        if (!v.localPath.isEmpty()) {
            QFile::remove(v.localPath);
        }
        m_cache->remove(v.fileId);
    }
    if (!victims.isEmpty()) {
        qCDebug(KINEMA)
            << "SubtitleController: evicted" << victims.size()
            << "subtitles to fit budget";
        Q_EMIT cacheChanged();
    }
}

void SubtitleController::rebuildCachedFileIds(const api::PlaybackKey& key)
{
    if (!m_cache || !key.isValid()) {
        m_cachedFileIds.clear();
        return;
    }
    m_cachedFileIds = m_cache->cachedFileIds(key);
}

void SubtitleController::reconcileCacheOnStartup()
{
    if (!m_cache) {
        return;
    }

    const auto entries = m_cache->all();
    int orphanRows = 0;
    QSet<QString> referenced;

    // Drop rows whose local_path no longer exists.
    for (const auto& e : entries) {
        if (e.localPath.isEmpty() || !QFile::exists(e.localPath)) {
            m_cache->remove(e.fileId);
            ++orphanRows;
        } else {
            referenced.insert(QFileInfo(e.localPath).canonicalFilePath());
        }
    }

    // Walk the cache dir and remove files not referenced by any row.
    QDir subsDir = core::cache::subtitlesDir();
    int orphanFiles = 0;
    if (subsDir.exists()) {
        QDirIterator it(subsDir.absolutePath(),
            QDir::Files | QDir::NoDotAndDotDot,
            QDirIterator::Subdirectories);
        while (it.hasNext()) {
            it.next();
            const auto canonical = it.fileInfo().canonicalFilePath();
            if (!referenced.contains(canonical)) {
                QFile::remove(it.filePath());
                ++orphanFiles;
            }
        }
    }

    qCDebug(KINEMA)
        << "SubtitleController: reconcile dropped" << orphanRows
        << "orphan rows and" << orphanFiles << "orphan files";

    evictIfOverBudget();

    if (orphanRows > 0 || orphanFiles > 0) {
        Q_EMIT cacheChanged();
    }
}

void SubtitleController::clearAllCachedSubtitles()
{
    if (!m_cache) {
        return;
    }
    const auto entries = m_cache->all();
    for (const auto& e : entries) {
        if (!e.localPath.isEmpty()) {
            QFile::remove(e.localPath);
        }
    }
    m_cache->clearAll();
    m_cachedFileIds.clear();
    Q_EMIT cacheChanged();
}

} // namespace kinema::controllers
