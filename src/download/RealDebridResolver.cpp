// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "download/RealDebridResolver.h"

#include "api/RealDebridClient.h"
#include "core/HttpError.h"
#include "core/Magnet.h"

#include <KLocalizedString>

#include <QCoro/QCoroSignal>
#include <QFileInfo>
#include <QTimer>

#include <chrono>

namespace kinema::download {

namespace {

using namespace std::chrono_literals;

constexpr int kTorrentReadyTimeoutMs = 90'000;
constexpr int kPollIntervalMs = 1'000;

QCoro::Task<void> sleepMs(int ms)
{
    QTimer t;
    t.setSingleShot(true);
    t.start(ms);
    co_await qCoro(&t, &QTimer::timeout);
}

/// Score how well an RD file matches the asset hint. Higher is better.
int score(const QString& path,
    qint64 sizeBytes,
    const api::AssetRef& ref)
{
    int s = 0;

    if (sizeBytes >= 50LL * 1024LL * 1024LL) {
        s += 2;
    }

    const auto suffix = QFileInfo(path).suffix().toLower();
    static const QStringList kVideo {
        QStringLiteral("mkv"), QStringLiteral("mp4"),
        QStringLiteral("m4v"), QStringLiteral("avi"),
        QStringLiteral("mov"), QStringLiteral("ts"),
    };
    if (kVideo.contains(suffix)) {
        s += 4;
    }

    if (!ref.fileNameHint.isEmpty()) {
        const auto base = QFileInfo(path).fileName();
        if (base.compare(ref.fileNameHint, Qt::CaseInsensitive) == 0) {
            s += 16;
        } else if (base.contains(ref.fileNameHint, Qt::CaseInsensitive)) {
            s += 8;
        }
    }

    if (ref.key.season.has_value() && ref.key.episode.has_value()) {
        const auto s2 = QStringLiteral("S%1").arg(*ref.key.season,
            2, 10, QLatin1Char('0'));
        const auto s2u = QStringLiteral("%1x").arg(*ref.key.season,
            2, 10, QLatin1Char('0'));
        const auto e2 = QStringLiteral("E%1").arg(*ref.key.episode,
            2, 10, QLatin1Char('0'));
        if (path.contains(s2 + e2, Qt::CaseInsensitive)) {
            s += 8;
        } else if (path.contains(s2u, Qt::CaseInsensitive)
            && path.contains(QString::number(*ref.key.episode))) {
            s += 4;
        }
    }
    return s;
}

/// Pick the best file id from a torrent-info response.
int chooseFileId(const QList<api::RdTorrentFile>& files,
    const api::AssetRef& ref)
{
    int bestScore = -1;
    int bestId = -1;
    qint64 bestSize = -1;

    for (const auto& f : files) {
        const int sc = score(f.path, f.bytes, ref);
        if (sc > bestScore || (sc == bestScore && f.bytes > bestSize)) {
            bestScore = sc;
            bestSize = f.bytes;
            bestId = f.id;
        }
    }
    return bestId;
}

} // namespace

RealDebridResolver::RealDebridResolver(api::RealDebridClient& rd, QObject* parent)
    : QObject(parent)
    , m_rd(rd)
{
}

QCoro::Task<void> RealDebridResolver::cleanup(QString rdTorrentId)
{
    if (rdTorrentId.isEmpty()) {
        co_return;
    }
    try {
        co_await m_rd.deleteTorrent(rdTorrentId);
    } catch (...) {
        // best-effort
    }
}

QCoro::Task<ResolvedRdLink> RealDebridResolver::resolve(api::AssetRef ref)
{
    if (ref.infoHash.isEmpty()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("Real-Debrid resolution requires an info hash."));
    }

    // RD deprecated the `/torrents/instantAvailability/` endpoint
    // (it returns 403 / empty objects in practice), so we no longer
    // probe cache state up front \u2014 the result was unused anyway.
    // We go straight to addMagnet; if RD has the bytes already the
    // status flips to `downloaded` near-instantly and the loops
    // below return in one tick.

    // Step 1: Add the magnet to the user's torrents.
    const auto magnet = core::magnet::build(ref.infoHash, ref.releaseName);
    const auto added = co_await m_rd.addMagnet(magnet);

    // Step 2: Wait until RD has populated the file list.
    api::RdTorrentInfo info;
    int waitedMs = 0;
    while (true) {
        info = co_await m_rd.torrentInfo(added.id);
        if (!info.files.isEmpty()) {
            break;
        }
        if (waitedMs >= kTorrentReadyTimeoutMs) {
            throw core::HttpError(core::HttpError::Kind::Json, 0,
                i18n("Real-Debrid did not produce a file list in time."));
        }
        co_await sleepMs(kPollIntervalMs);
        waitedMs += kPollIntervalMs;
    }

    // Step 3: choose file id (uses our scoring) and ask RD to select it.
    int chosenId = -1;
    if (ref.fileIndex >= 0) {
        // Torrentio reports `fileIdx` as a 0-based index; RD ids are
        // 1-based and may be re-numbered if RD's file list excludes
        // some entries. Try the 1-based equivalent first, then fall
        // back to scoring.
        const int candidate = ref.fileIndex + 1;
        for (const auto& f : info.files) {
            if (f.id == candidate) {
                chosenId = candidate;
                break;
            }
        }
    }
    if (chosenId < 0) {
        chosenId = chooseFileId(info.files, ref);
    }
    if (chosenId < 0) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("Real-Debrid did not return a usable file."));
    }

    co_await m_rd.selectFiles(added.id, QList<int> { chosenId });

    // Step 4: Wait until RD has produced a link for the selected file.
    waitedMs = 0;
    while (true) {
        info = co_await m_rd.torrentInfo(added.id);
        if (!info.links.isEmpty()
            && (info.status == QLatin1String("downloaded")
                || info.status == QLatin1String("queued")
                || info.status == QLatin1String("compressing")
                || info.status == QLatin1String("downloading"))) {
            break;
        }
        if (waitedMs >= kTorrentReadyTimeoutMs) {
            throw core::HttpError(core::HttpError::Kind::Json, 0,
                i18n("Real-Debrid did not produce a download link in time."));
        }
        co_await sleepMs(kPollIntervalMs);
        waitedMs += kPollIntervalMs;
    }

    // Step 5: unrestrict the (first) hoster link.
    const QUrl link = info.links.first();
    const auto unrestricted = co_await m_rd.unrestrictLink(link);

    ResolvedRdLink out;
    out.downloadUrl = unrestricted.download;
    out.fileSize = unrestricted.fileSize;
    out.fileName = unrestricted.filename.isEmpty()
        ? ref.fileNameHint
        : unrestricted.filename;
    out.rdTorrentId = added.id;
    co_return out;
}

} // namespace kinema::download
