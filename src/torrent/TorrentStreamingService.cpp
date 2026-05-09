// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "torrent/TorrentStreamingService.h"

#include "config/TorrentStreamingSettings.h"
#include "core/Magnet.h"
#include "core/TorrentCache.h"
#include "kinema_debug.h"
#include "torrent/LocalStreamServer.h"
#include "torrent/MediaFileSelector.h"

#include <KLocalizedString>

#include <QCoro/QCoroSignal>

#include <QDateTime>
#include <QFile>
#include <QFileInfo>
#include <QTimer>
#include <QUuid>

#include <libtorrent/add_torrent_params.hpp>
#include <libtorrent/error_code.hpp>
#include <libtorrent/file_storage.hpp>
#include <libtorrent/magnet_uri.hpp>
#include <libtorrent/session.hpp>
#include <libtorrent/settings_pack.hpp>
#include <libtorrent/torrent_handle.hpp>
#include <libtorrent/torrent_info.hpp>

#include <chrono>
#include <stdexcept>

namespace kinema::torrent {

namespace lt = libtorrent;
using namespace std::chrono_literals;

namespace {
constexpr int kMetadataTimeoutMs = 60'000;
constexpr int kStartupTimeoutMs = 180'000;
constexpr int kRangeTimeoutMs = 120'000;

QCoro::Task<void> sleepMs(int ms)
{
    QTimer timer;
    timer.setSingleShot(true);
    timer.start(ms);
    co_await qCoro(&timer, &QTimer::timeout);
}

QString normalizedHash(QString hash)
{
    return hash.trimmed().toLower();
}

qint64 mibToBytes(int mib)
{
    return static_cast<qint64>(mib) * 1024LL * 1024LL;
}

std::runtime_error runtimeError(const QString& msg)
{
    return std::runtime_error(msg.toStdString());
}

QString fileNameForUrl(const QString& path)
{
    const auto name = QFileInfo(path).fileName();
    return name.isEmpty() ? QStringLiteral("stream") : name;
}
}

struct TorrentStreamingService::Private {
    Private(TorrentStreamingService* q,
        core::TorrentCache& cacheIn,
        const config::TorrentStreamingSettings& settingsIn)
        : cache(cacheIn)
        , settings(settingsIn)
        , server(*q, q)
        , idleTimer(q)
    {
        lt::settings_pack pack;
        applyTransferSettings(pack);
        session.apply_settings(pack);

        idleTimer.setInterval(60'000);
        QObject::connect(&idleTimer, &QTimer::timeout, q, [this] {
            stopIdleSessions();
        });
        idleTimer.start();
    }

    struct SessionState {
        QString infoHash;
        QString token;
        lt::torrent_handle handle;
        SelectedMediaFile selected;
        FilePieceLayout layout;
        QString filePath;
        QDateTime lastActivity = QDateTime::currentDateTimeUtc();
        api::PlaybackKey key;
    };

    void applyTransferSettings(lt::settings_pack& pack) const
    {
        const int dl = settings.maxDownloadRateKiB();
        const int ul = settings.maxUploadRateKiB();
        pack.set_int(lt::settings_pack::download_rate_limit,
            dl <= 0 ? 0 : dl * 1024);
        pack.set_int(lt::settings_pack::upload_rate_limit,
            ul <= 0 ? 0 : ul * 1024);
    }

    SessionState* byToken(const QString& token)
    {
        const auto hash = tokenToHash.value(token);
        if (hash.isEmpty()) {
            return nullptr;
        }
        auto it = sessions.find(hash);
        return it == sessions.end() ? nullptr : &it.value();
    }

    const SessionState* byToken(const QString& token) const
    {
        const auto hash = tokenToHash.value(token);
        if (hash.isEmpty()) {
            return nullptr;
        }
        auto it = sessions.constFind(hash);
        return it == sessions.constEnd() ? nullptr : &it.value();
    }

    void stopHash(const QString& hash)
    {
        const QString h = normalizedHash(hash);
        auto it = sessions.find(h);
        if (it == sessions.end()) {
            return;
        }
        if (it->handle.is_valid()) {
            it->handle.pause();
            session.remove_torrent(it->handle);
        }
        tokenToHash.remove(it->token);
        cache.markInactive(h);
        sessions.erase(it);
        cache.enforceBudget();
    }

    void stopIdleSessions()
    {
        const auto now = QDateTime::currentDateTimeUtc();
        const auto idleSecs = settings.idleStopMinutes() * 60;
        QStringList stop;
        for (auto it = sessions.cbegin(); it != sessions.cend(); ++it) {
            if (it->lastActivity.secsTo(now) >= idleSecs) {
                stop.append(it.key());
            }
        }
        for (const auto& hash : stop) {
            stopHash(hash);
        }
    }

    core::TorrentCache& cache;
    const config::TorrentStreamingSettings& settings;
    lt::session session;
    LocalStreamServer server;
    QTimer idleTimer;
    QHash<QString, SessionState> sessions;
    QHash<QString, QString> tokenToHash;
};

TorrentStreamingService::TorrentStreamingService(core::TorrentCache& cache,
    const config::TorrentStreamingSettings& settings, QObject* parent)
    : QObject(parent)
    , d(std::make_unique<Private>(this, cache, settings))
{
}

TorrentStreamingService::~TorrentStreamingService()
{
    stopAll();
}

QCoro::Task<PreparedSession> TorrentStreamingService::prepareSession(
    const api::Stream& stream, const api::PlaybackContext& ctx)
{
    if (stream.infoHash.isEmpty()) {
        throw runtimeError(i18nc("@info:status",
            "This stream has no magnet info hash."));
    }

    const QString hash = normalizedHash(stream.infoHash);
    d->cache.markActive(hash);

    auto it = d->sessions.find(hash);
    if (it == d->sessions.end()) {
        const QString magnet = core::magnet::build(hash, stream.releaseName);
        lt::error_code ec;
        lt::add_torrent_params atp = lt::parse_magnet_uri(
            magnet.toStdString(), ec);
        if (ec) {
            d->cache.markInactive(hash);
            throw runtimeError(i18nc("@info:status",
                "Could not parse the magnet link: %1",
                QString::fromStdString(ec.message())));
        }
        atp.save_path = d->cache.torrentDir(hash).absolutePath().toStdString();

        lt::torrent_handle handle = d->session.add_torrent(atp, ec);
        if (ec || !handle.is_valid()) {
            d->cache.markInactive(hash);
            throw runtimeError(i18nc("@info:status",
                "Could not add the torrent: %1",
                QString::fromStdString(ec.message())));
        }
        handle.resume();

        Private::SessionState state;
        state.infoHash = hash;
        state.token = QUuid::createUuid().toString(QUuid::WithoutBraces);
        state.handle = handle;
        state.key = ctx.key;
        state.lastActivity = QDateTime::currentDateTimeUtc();
        d->tokenToHash.insert(state.token, hash);
        it = d->sessions.insert(hash, std::move(state));
    }

    auto& state = it.value();
    state.lastActivity = QDateTime::currentDateTimeUtc();
    state.key = ctx.key;

    if (state.selected.index < 0) {
        Q_EMIT statusMessage(i18nc("@info:status",
            "Fetching torrent metadata for “%1”…",
            ctx.title.isEmpty() ? stream.releaseName : ctx.title), 0);

        const auto start = QDateTime::currentMSecsSinceEpoch();
        while (true) {
            if (!state.handle.is_valid()) {
                throw runtimeError(i18nc("@info:status",
                    "Torrent session ended before metadata was available."));
            }
            const auto ti = state.handle.torrent_file();
            if (ti) {
                const auto& fs = ti->files();
                QVector<TorrentFileEntry> files;
                files.reserve(fs.num_files());
                for (int i = 0; i < fs.num_files(); ++i) {
                    const lt::file_index_t idx(i);
                    files.append({ i,
                        QString::fromStdString(fs.file_path(idx)),
                        fs.file_size(idx) });
                }
                auto sel = selectMediaFile(files, ctx);
                if (!sel.ok()) {
                    throw runtimeError(sel.error);
                }
                state.selected = *sel.file;
                const lt::file_index_t fidx(state.selected.index);
                state.layout.fileOffset = fs.file_offset(fidx);
                state.layout.fileSize = fs.file_size(fidx);
                state.layout.pieceSize = ti->piece_length();
                state.layout.pieceCount = ti->num_pieces();
                state.filePath = d->cache.torrentDir(hash)
                    .absoluteFilePath(state.selected.path);

                std::vector<lt::download_priority_t> priorities(
                    fs.num_files(), lt::dont_download);
                priorities[state.selected.index] = lt::top_priority;
                state.handle.prioritize_files(priorities);
                break;
            }
            if (QDateTime::currentMSecsSinceEpoch() - start > kMetadataTimeoutMs) {
                throw runtimeError(i18nc("@info:status",
                    "Timed out while fetching torrent metadata."));
            }
            co_await sleepMs(250);
        }
    }

    const auto windows = startupPieceWindows(state.layout,
        mibToBytes(d->settings.startupBufferMiB()),
        mibToBytes(d->settings.tailBufferMiB()));
    int deadline = 0;
    for (const auto& w : windows) {
        for (int p = w.first; p <= w.last; ++p) {
            state.handle.set_piece_deadline(lt::piece_index_t(p),
                deadline, lt::torrent_handle::alert_when_available);
            deadline += 10;
        }
    }

    Q_EMIT statusMessage(i18nc("@info:status",
        "Buffering torrent stream…"), 0);

    const auto initial = clampRange(0,
        mibToBytes(d->settings.startupBufferMiB()), state.layout.fileSize);
    if (initial.isValid()) {
        const bool ready = co_await ensureRange(state.token, initial);
        if (!ready) {
            throw runtimeError(i18nc("@info:status",
                "Timed out while buffering the torrent stream."));
        }
    }

    d->cache.touch(hash);
    d->cache.enforceBudget();

    PreparedSession ps;
    ps.token = state.token;
    ps.fileName = fileNameForUrl(state.selected.path);
    ps.fileSize = state.layout.fileSize;
    ps.infoHash = state.infoHash;
    co_return ps;
}

QCoro::Task<QUrl> TorrentStreamingService::prepare(const api::Stream& stream,
    const api::PlaybackContext& ctx)
{
    if (!d->server.listen()) {
        throw runtimeError(i18nc("@info:status",
            "Could not start the local torrent streaming server."));
    }
    const auto ps = co_await prepareSession(stream, ctx);
    co_return d->server.urlForToken(ps.token, ps.fileName);
}

QCoro::Task<bool> TorrentStreamingService::ensureRange(const QString& token,
    ByteRange range)
{
    auto* state = d->byToken(token);
    if (!state || !range.isValid()) {
        co_return false;
    }
    state->lastActivity = QDateTime::currentDateTimeUtc();

    const auto urgent = readaheadRange(range.start, range.endInclusive,
        mibToBytes(d->settings.readaheadMiB()), state->layout.fileSize);
    const auto pieces = pieceRangeForBytes(state->layout, urgent);
    const auto requiredPieces = pieceRangeForBytes(state->layout, range);
    if (!pieces.isValid() || !requiredPieces.isValid()) {
        co_return false;
    }

    int deadline = 0;
    for (int p = pieces.first; p <= pieces.last; ++p) {
        state->handle.set_piece_deadline(lt::piece_index_t(p),
            deadline, lt::torrent_handle::alert_when_available);
        deadline += 5;
    }

    const auto start = QDateTime::currentMSecsSinceEpoch();
    while (true) {
        bool allReady = true;
        for (int p = requiredPieces.first; p <= requiredPieces.last; ++p) {
            if (!state->handle.have_piece(lt::piece_index_t(p))) {
                allReady = false;
                break;
            }
        }
        if (allReady) {
            co_return true;
        }
        if (QDateTime::currentMSecsSinceEpoch() - start > kRangeTimeoutMs) {
            co_return false;
        }
        co_await sleepMs(100);
    }
}

QByteArray TorrentStreamingService::readRange(const QString& token,
    ByteRange range) const
{
    const auto* state = d->byToken(token);
    if (!state || !range.isValid()) {
        return {};
    }
    QFile file(state->filePath);
    if (!file.open(QIODevice::ReadOnly) || !file.seek(range.start)) {
        return {};
    }
    return file.read(range.endInclusive - range.start + 1);
}

qint64 TorrentStreamingService::fileSizeForToken(const QString& token) const
{
    const auto* state = d->byToken(token);
    return state ? state->layout.fileSize : 0;
}

QString TorrentStreamingService::fileNameForToken(const QString& token) const
{
    const auto* state = d->byToken(token);
    return state ? QFileInfo(state->selected.path).fileName() : QString {};
}

void TorrentStreamingService::touchToken(const QString& token)
{
    auto* state = d->byToken(token);
    if (!state) {
        return;
    }
    state->lastActivity = QDateTime::currentDateTimeUtc();
    d->cache.touch(state->infoHash);
}

void TorrentStreamingService::stopInfoHash(const QString& infoHash)
{
    d->stopHash(infoHash);
}

void TorrentStreamingService::stopForContext(const api::PlaybackContext& ctx)
{
    if (!ctx.streamRef.infoHash.isEmpty()) {
        stopInfoHash(ctx.streamRef.infoHash);
    }
}

void TorrentStreamingService::stopAll()
{
    const auto hashes = d->sessions.keys();
    for (const auto& hash : hashes) {
        d->stopHash(hash);
    }
}

} // namespace kinema::torrent
