// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/sources/RealDebridResolver.h"

#include "api/RealDebridClient.h"
#include "core/io/HttpError.h"
#include "core/util/Magnet.h"
#include "playback/sources/DebridFilePicker.h"

#include <KLocalizedString>

#include <QCoro/QCoroSignal>
#include <QTimer>

#include <chrono>

namespace kinema::playback::sources {

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

/// Pick the best file id from an RD torrent-info response. For normal
/// Torrentio clicks `ref.fileIndex` is usually RD's id - 1, but series
/// auto-next can feed us an index from the already-resolved RD catalog.
/// That catalog index is positional, while RD's file ids may skip / drift
/// when non-video rows are present. Prefer strong filename / episode
/// evidence when available; fall back to the historical id mapping only
/// when the scorer cannot distinguish the target.
int chooseFileId(const QList<domain::RdTorrentFile>& files,
    const domain::AssetRef& ref)
{
    QList<picker::Candidate> candidates;
    candidates.reserve(files.size());
    int bestIdx = -1;
    int bestScore = -1;
    for (int i = 0; i < files.size(); ++i) {
        const auto& f = files[i];
        candidates.append({ f.path, f.bytes });
        const int sc = picker::score(f.path, f.bytes, ref);
        if (sc > bestScore) {
            bestScore = sc;
            bestIdx = i;
        }
    }

    // Generic playable video rows score up to 6 (size + video suffix).
    // Anything higher means the filename hint or SxxExx/1x episode token
    // matched the requested asset, which is safer than assuming RD ids are
    // positional. This is the critical path for embedded auto-next.
    if (bestIdx >= 0 && bestScore > 6) {
        return files[bestIdx].id;
    }

    if (ref.fileIndex >= 0) {
        const int candidate = ref.fileIndex + 1;
        for (const auto& f : files) {
            if (f.id == candidate) {
                return candidate;
            }
        }
    }

    const int idx = picker::chooseIndex(candidates, ref);
    return idx < 0 ? -1 : files[idx].id;
}

} // namespace

RealDebridResolver::RealDebridResolver(api::RealDebridClient& rd, QObject* parent)
    : DebridResolver(parent)
    , m_rd(rd)
{
}

QCoro::Task<void> RealDebridResolver::cleanup(QString providerTorrentId)
{
    if (providerTorrentId.isEmpty()) {
        co_return;
    }
    try {
        co_await m_rd.deleteTorrent(providerTorrentId);
    } catch (...) {
        // best-effort
    }
}

QCoro::Task<ResolvedDebridLink> RealDebridResolver::resolve(domain::AssetRef ref)
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
    domain::RdTorrentInfo info;
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

    // Step 3: choose file id and ask RD to select it.
    int chosenId = chooseFileId(info.files, ref);
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

    ResolvedDebridLink out;
    out.downloadUrl = unrestricted.download;
    out.fileSize = unrestricted.fileSize;
    out.fileName = unrestricted.filename.isEmpty()
        ? ref.fileNameHint
        : unrestricted.filename;
    out.providerTorrentId = added.id;

    // Preserve the full magnet file list so the asset session can
    // surface it to series auto-next without a libtorrent session.
    // RD uses 1-based ids and may re-number entries; we flatten to
    // 0-based positional indices here so callers don't have to know
    // about provider quirks.
    out.files.reserve(info.files.size());
    for (int i = 0; i < info.files.size(); ++i) {
        out.files.append(torrent::TorrentFileEntry {
            i, info.files[i].path, info.files[i].bytes });
    }
    co_return out;
}

} // namespace kinema::playback::sources
