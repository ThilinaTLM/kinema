// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "torrent/TorrentStreamingService.h"

#include "config/TorrentStreamingSettings.h"
#include "core/util/Magnet.h"
#include "core/persistence/TorrentCache.h"
#include "kinema_log_torrent.h"
#include "playback/policy/MediaFileSelectionPolicy.h"
#include "playback/torrent/LibtorrentClient.h"
#include "torrent/LocalStreamServer.h"
#include "torrent/TorrentFileEntry.h"

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
#include <libtorrent/hex.hpp>
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

/// Short prefix of an info hash for log lines.
QString shortHash(const QString& h)
{
    return h.left(8);
}

QVector<TorrentFileEntry> torrentFileEntries(
    const std::shared_ptr<const lt::torrent_info>& ti)
{
    QVector<TorrentFileEntry> files;
    if (!ti) {
        return files;
    }

    const auto& fs = ti->files();
    files.reserve(fs.num_files());
    for (int i = 0; i < fs.num_files(); ++i) {
        const lt::file_index_t idx(i);
        files.append({ i,
            QString::fromStdString(fs.file_path(idx)),
            fs.file_size(idx) });
    }
    return files;
}

std::optional<domain::MediaFileEntry> requestedFileSelection(
    const QVector<TorrentFileEntry>& files,
    const domain::Stream& stream,
    const domain::PlaybackContext& ctx,
    QString* error)
{
    if (stream.fileIndex >= 0) {
        for (const auto& f : files) {
            if (f.index == stream.fileIndex) {
                return domain::MediaFileEntry { f.index, f.path, f.size, true };
            }
        }
        if (error) {
            *error = i18nc("@info:status",
                "The selected episode file is no longer present in this torrent.");
        }
        return std::nullopt;
    }

    const auto sel = playback::policy::selectMediaFile(files, ctx);
    if (!sel.ok()) {
        if (error) {
            *error = sel.error;
        }
        return std::nullopt;
    }
    return sel.file;
}
}

struct TorrentStreamingService::Private {
    Private(TorrentStreamingService* q,
        core::TorrentCache& cacheIn,
        const config::TorrentStreamingSettings& settingsIn,
        kinema::playback::torrent::LibtorrentClient& clientIn)
        : owner(q)
        , cache(cacheIn)
        , settings(settingsIn)
        , client(clientIn)
        , server(*q, q)
        , idleTimer(q)
    {
        // The LibtorrentClient is already started by
        // `ensureStarted()` before this Private is built — it owns
        // the lt::session, settings, alert pump, and stats timer.
        // Alert decoding is wired in `TorrentStreamingService` via
        // direct Qt signal connections so per-session bookkeeping
        // updates stay on the same thread as the rest of TSS.
        idleTimer.setInterval(60'000);
        QObject::connect(&idleTimer, &QTimer::timeout, q, [this] {
            stopIdleSessions();
        });
        idleTimer.start();
    }

    struct LiveStats {
        qint64 doneBytes = 0;
        qint64 ratePayloadBps = 0;
        int    numPeers = 0;
        int    numSeeds = 0;
        int    etaSeconds = -1;
        bool   finished = false;
    };

    struct SessionState {
        QString infoHash;
        QString token;
        lt::torrent_handle handle;
        domain::MediaFileEntry selected;
        FilePieceLayout layout;
        QString filePath;
        QDateTime lastActivity = QDateTime::currentDateTimeUtc();
        domain::PlaybackKey key;
        LiveStats lastStats;
        bool keepAlive = false;
    };

    lt::session& ltSession() { return *client.session(); }

    void refreshStatsTimerRunning()
    {
        client.setActiveHandleCount(sessions.size());
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

    void stopHash(const QString& hash, const char* reason)
    {
        const QString h = normalizedHash(hash);
        auto it = sessions.find(h);
        if (it == sessions.end()) {
            return;
        }
        qCInfo(KINEMA_TORRENT).nospace()
            << "[hash=" << shortHash(h) << "] stopping ("
            << reason << "); was keepAlive=" << it->keepAlive;
        if (it->handle.is_valid()) {
            it->handle.pause();
            ltSession().remove_torrent(it->handle);
        }
        tokenToHash.remove(it->token);
        cache.markInactive(h);
        sessions.erase(it);
        cache.enforceBudget();
        refreshStatsTimerRunning();
    }

    void stopIdleSessions()
    {
        const auto now = QDateTime::currentDateTimeUtc();
        const auto idleSecs = settings.idleStopMinutes() * 60;
        QStringList stop;
        for (auto it = sessions.cbegin(); it != sessions.cend(); ++it) {
            // Pinned ("Save offline") sessions opt out of idle-stop
            // so the swarm keeps pulling pieces while the user is
            // off doing other things.
            if (it->keepAlive) {
                continue;
            }
            if (it->lastActivity.secsTo(now) >= idleSecs) {
                stop.append(it.key());
            }
        }
        if (!stop.isEmpty()) {
            qCInfo(KINEMA_TORRENT) << "idle-stopping" << stop.size()
                                   << "session(s) after" << idleSecs
                                   << "s of inactivity";
        }
        for (const auto& hash : stop) {
            stopHash(hash, "idle");
        }
    }

    TorrentStreamingService* owner;
    core::TorrentCache& cache;
    const config::TorrentStreamingSettings& settings;
    kinema::playback::torrent::LibtorrentClient& client;
    LocalStreamServer server;
    QTimer idleTimer;
    QHash<QString, SessionState> sessions;
    QHash<QString, QString> tokenToHash;
};

TorrentStreamingService::TorrentStreamingService(core::TorrentCache& cache,
    const config::TorrentStreamingSettings& settings, QObject* parent)
    : QObject(parent)
    , d(nullptr)
    , m_cache(&cache)
    , m_settings(&settings)
    , m_client(std::make_unique<kinema::playback::torrent::LibtorrentClient>(
          settings, this))
{
    // Lazy-init contract: do not spin up libtorrent here. The
    // LibtorrentClient is constructed dormant; `Private` (per-asset
    // bookkeeping + legacy LocalStreamServer + idle timer) is built
    // by `ensureStarted()` on the first call that genuinely needs
    // a libtorrent session (`prepare()` / `prepareSession()`).
    // Until then this instance is dormant and every other public
    // method is a cheap no-op. Lets users on Real-Debrid / AllDebrid
    // boot without paying the libtorrent cost.
    qCDebug(KINEMA_TORRENT)
        << "TorrentStreamingService constructed (dormant)";
}

TorrentStreamingService::TorrentStreamingService(StubTag, QObject* parent)
    : QObject(parent)
    , d(nullptr)
{
}

void TorrentStreamingService::ensureStarted()
{
    if (d) {
        return;
    }
    if (!m_cache || !m_settings || !m_client) {
        // StubTag instances have no backing services; refuse to
        // lazy-start so test doubles stay dormant even if a path
        // accidentally reaches a real entry point.
        return;
    }
    qCInfo(KINEMA_TORRENT)
        << "starting libtorrent session on first use";
    m_client->ensureStarted();
    d = std::make_unique<Private>(this, *m_cache, *m_settings,
        *m_client);

    // Decode the LibtorrentClient alert stream into TSS's existing
    // signal surface. Connections live for the lifetime of `this`
    // because the client is parented to TSS.
    connect(m_client.get(),
        &kinema::playback::torrent::LibtorrentClient::statsUpdated,
        this, &TorrentStreamingService::onLibtorrentStats);
    connect(m_client.get(),
        &kinema::playback::torrent::LibtorrentClient::torrentFinished,
        this, &TorrentStreamingService::onLibtorrentFinished);
    connect(m_client.get(),
        &kinema::playback::torrent::LibtorrentClient::torrentFailed,
        this, &TorrentStreamingService::onLibtorrentFailed);
}

TorrentStreamingService::~TorrentStreamingService()
{
    if (d) {
        stopAll();
    }
}

QCoro::Task<PreparedSession> TorrentStreamingService::prepareSession(
    const domain::Stream& stream, const domain::PlaybackContext& ctx,
    PrepareMode mode)
{
    ensureStarted();
    if (!d) {
        // StubTag instance reaching the real method; refuse rather
        // than crash. Production always has `d` after
        // `ensureStarted()`.
        throw runtimeError(i18nc("@info:status",
            "Torrent streaming is not available in this build."));
    }
    if (stream.infoHash.isEmpty()) {
        throw runtimeError(i18nc("@info:status",
            "This stream has no magnet info hash."));
    }

    const QString hash = normalizedHash(stream.infoHash);
    d->cache.markActive(hash);

    qCInfo(KINEMA_TORRENT).nospace()
        << "prepareSession[hash=" << shortHash(hash)
        << " mode="
        << (mode == PrepareMode::Background ? "background" : "streaming")
        << " release=\"" << stream.releaseName
        << "\" title=\"" << ctx.title << "\"]";

    auto it = d->sessions.find(hash);
    if (it == d->sessions.end()) {
        const QString magnet = core::magnet::build(hash, stream.releaseName);
        lt::error_code ec;
        lt::add_torrent_params atp = lt::parse_magnet_uri(
            magnet.toStdString(), ec);
        if (ec) {
            d->cache.markInactive(hash);
            qCWarning(KINEMA_TORRENT).nospace()
                << "[hash=" << shortHash(hash) << "] parse_magnet_uri: "
                << QString::fromStdString(ec.message());
            throw runtimeError(i18nc("@info:status",
                "Could not parse the magnet link: %1",
                QString::fromStdString(ec.message())));
        }
        const auto savePath = d->cache.torrentDir(hash).absolutePath();
        atp.save_path = savePath.toStdString();

        lt::torrent_handle handle = d->ltSession().add_torrent(atp, ec);
        if (ec || !handle.is_valid()) {
            d->cache.markInactive(hash);
            qCWarning(KINEMA_TORRENT).nospace()
                << "[hash=" << shortHash(hash) << "] add_torrent failed: "
                << QString::fromStdString(ec.message());
            throw runtimeError(i18nc("@info:status",
                "Could not add the torrent: %1",
                QString::fromStdString(ec.message())));
        }
        handle.resume();
        qCInfo(KINEMA_TORRENT).nospace()
            << "[hash=" << shortHash(hash) << "] added; save_path=\""
            << savePath << "\"";

        Private::SessionState state;
        state.infoHash = hash;
        state.token = QUuid::createUuid().toString(QUuid::WithoutBraces);
        state.handle = handle;
        state.key = ctx.key;
        state.lastActivity = QDateTime::currentDateTimeUtc();
        d->tokenToHash.insert(state.token, hash);
        it = d->sessions.insert(hash, std::move(state));
        d->refreshStatsTimerRunning();
    }

    auto& state = it.value();
    state.lastActivity = QDateTime::currentDateTimeUtc();
    state.key = ctx.key;

    if (state.selected.index < 0) {
        Q_EMIT statusMessage(i18nc("@info:status",
            "Fetching torrent metadata for “%1”…",
            ctx.title.isEmpty() ? stream.releaseName : ctx.title), 0);
    }

    const auto start = QDateTime::currentMSecsSinceEpoch();
    while (true) {
        if (!state.handle.is_valid()) {
            throw runtimeError(i18nc("@info:status",
                "Torrent session ended before metadata was available."));
        }
        const auto ti = state.handle.torrent_file();
        if (ti) {
            const auto files = torrentFileEntries(ti);
            QString selectionError;
            const auto requested = requestedFileSelection(files,
                stream, ctx, &selectionError);
            if (!requested) {
                throw runtimeError(selectionError);
            }

            const bool changedFile = state.selected.index >= 0
                && state.selected.index != requested->index;
            if (state.selected.index < 0 || changedFile) {
                const auto& fs = ti->files();
                state.selected = *requested;
                const lt::file_index_t fidx(state.selected.index);
                state.layout.fileOffset = fs.file_offset(fidx);
                state.layout.fileSize = fs.file_size(fidx);
                state.layout.pieceSize = ti->piece_length();
                state.layout.pieceCount = ti->num_pieces();
                state.filePath = d->cache.torrentDir(hash)
                    .absoluteFilePath(state.selected.path);
                state.handle.clear_piece_deadlines();

                std::vector<lt::download_priority_t> priorities(
                    fs.num_files(), lt::dont_download);
                priorities[state.selected.index] = lt::top_priority;
                state.handle.prioritize_files(priorities);

                if (changedFile) {
                    d->tokenToHash.remove(state.token);
                    state.token = QUuid::createUuid().toString(
                        QUuid::WithoutBraces);
                    d->tokenToHash.insert(state.token, hash);
                }

                qCInfo(KINEMA_TORRENT).nospace()
                    << "[hash=" << shortHash(hash)
                    << "] metadata ready; " << fs.num_files()
                    << " file(s); selected idx="
                    << state.selected.index << " path=\""
                    << state.selected.path << "\" size="
                    << state.layout.fileSize << " pieceSize="
                    << state.layout.pieceSize;
            }
            break;
        }
        if (QDateTime::currentMSecsSinceEpoch() - start > kMetadataTimeoutMs) {
            throw runtimeError(i18nc("@info:status",
                "Timed out while fetching torrent metadata."));
        }
        co_await sleepMs(250);
    }

    // Streaming mode pre-warms the head/tail piece windows so the
    // player can begin playback immediately. Background mode lets
    // libtorrent download the file in normal piece order — the user
    // is not waiting on the player here.
    if (mode == PrepareMode::Streaming) {
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

QCoro::Task<QUrl> TorrentStreamingService::prepare(const domain::Stream& stream,
    const domain::PlaybackContext& ctx)
{
    ensureStarted();
    if (!d) {
        throw runtimeError(i18nc("@info:status",
            "Torrent streaming is not available in this build."));
    }
    if (!d->server.listen()) {
        throw runtimeError(i18nc("@info:status",
            "Could not start the local torrent streaming server."));
    }
    const auto ps = co_await prepareSession(stream, ctx, PrepareMode::Streaming);
    co_return d->server.urlForToken(ps.token, ps.fileName);
}

QCoro::Task<bool> TorrentStreamingService::ensureRange(const QString& token,
    ByteRange range)
{
    if (!d) {
        co_return false;
    }
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
    if (!d) {
        return {};
    }
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
    if (!d) {
        return 0;
    }
    const auto* state = d->byToken(token);
    return state ? state->layout.fileSize : 0;
}

QString TorrentStreamingService::fileNameForToken(const QString& token) const
{
    if (!d) {
        return {};
    }
    const auto* state = d->byToken(token);
    return state ? QFileInfo(state->selected.path).fileName() : QString {};
}

void TorrentStreamingService::touchToken(const QString& token)
{
    if (!d) {
        return;
    }
    auto* state = d->byToken(token);
    if (!state) {
        return;
    }
    state->lastActivity = QDateTime::currentDateTimeUtc();
    d->cache.touch(state->infoHash);
}

QVector<TorrentFileEntry> TorrentStreamingService::filesForInfoHash(
    const QString& infoHash) const
{
    if (!d) {
        return {};
    }
    const QString h = normalizedHash(infoHash);
    const auto it = d->sessions.constFind(h);
    if (it == d->sessions.constEnd() || !it->handle.is_valid()) {
        return {};
    }
    return torrentFileEntries(it->handle.torrent_file());
}

void TorrentStreamingService::stopInfoHash(const QString& infoHash)
{
    if (!d) {
        return;
    }
    d->stopHash(infoHash, "explicit");
}

void TorrentStreamingService::stopForContext(const domain::PlaybackContext& ctx)
{
    if (!ctx.streamRef.infoHash.isEmpty()) {
        stopInfoHash(ctx.streamRef.infoHash);
    }
}

void TorrentStreamingService::stopAll()
{
    if (!d) {
        return;
    }
    const auto hashes = d->sessions.keys();
    for (const auto& hash : hashes) {
        d->stopHash(hash, "shutdown");
    }
}

void TorrentStreamingService::setKeepAlive(const QString& infoHash, bool on)
{
    if (!d) {
        return;
    }
    const QString h = normalizedHash(infoHash);
    auto it = d->sessions.find(h);
    if (it == d->sessions.end()) {
        return;
    }
    if (it->keepAlive == on) {
        return;
    }
    it->keepAlive = on;
    qCInfo(KINEMA_TORRENT).nospace()
        << "[hash=" << shortHash(h) << "] keepAlive=" << on;
}

void TorrentStreamingService::pauseInfoHash(const QString& infoHash)
{
    if (!d) {
        return;
    }
    const QString h = normalizedHash(infoHash);
    auto it = d->sessions.find(h);
    if (it == d->sessions.end() || !it->handle.is_valid()) {
        return;
    }
    it->handle.pause();
    qCInfo(KINEMA_TORRENT).nospace()
        << "[hash=" << shortHash(h) << "] pause (user)";
}

void TorrentStreamingService::resumeInfoHash(const QString& infoHash)
{
    if (!d) {
        return;
    }
    const QString h = normalizedHash(infoHash);
    auto it = d->sessions.find(h);
    if (it == d->sessions.end() || !it->handle.is_valid()) {
        return;
    }
    it->handle.resume();
    it->lastActivity = QDateTime::currentDateTimeUtc();
    qCInfo(KINEMA_TORRENT).nospace()
        << "[hash=" << shortHash(h) << "] resume (user)";
}

void TorrentStreamingService::promoteToFull(const QString& infoHash)
{
    if (!d) {
        return;
    }
    const QString h = normalizedHash(infoHash);
    auto it = d->sessions.find(h);
    if (it == d->sessions.end() || !it->handle.is_valid()) {
        return;
    }
    auto& state = it.value();
    // Clear every per-piece deadline. Passing 0 deadline + an
    // unrecognised flag clears the entry; libtorrent's documented
    // way is `clear_piece_deadlines()` (since 1.2).
    state.handle.clear_piece_deadlines();
    // Re-assert top priority on the selected file in case the
    // session was created by a path that left other priorities in
    // place.
    if (state.layout.pieceCount > 0) {
        const auto ti = state.handle.torrent_file();
        if (ti) {
            const auto& fs = ti->files();
            std::vector<lt::download_priority_t> priorities(
                fs.num_files(), lt::dont_download);
            if (state.selected.index >= 0
                && state.selected.index
                    < static_cast<int>(priorities.size())) {
                priorities[state.selected.index] = lt::top_priority;
            }
            state.handle.prioritize_files(priorities);
        }
    }
    state.keepAlive = true;
    state.lastActivity = QDateTime::currentDateTimeUtc();
    qCInfo(KINEMA_TORRENT).nospace()
        << "[hash=" << shortHash(h)
        << "] promoteToFull: deadlines cleared, keepAlive=on";
}

void TorrentStreamingService::onLibtorrentStats(const QString& infoHash,
    qint64 doneBytes, qint64 ratePayloadBps, int peers, int seeds,
    int etaSeconds, bool finished)
{
    if (!d) {
        return;
    }
    auto sit = d->sessions.find(infoHash);
    if (sit == d->sessions.end()) {
        return;
    }
    sit->lastStats.doneBytes = doneBytes;
    sit->lastStats.ratePayloadBps = ratePayloadBps;
    sit->lastStats.numPeers = peers;
    sit->lastStats.numSeeds = seeds;
    sit->lastStats.etaSeconds = etaSeconds;
    sit->lastStats.finished = finished;
    Q_EMIT torrentStatsChanged(infoHash, doneBytes, ratePayloadBps,
        peers, seeds, etaSeconds);
}

void TorrentStreamingService::onLibtorrentFinished(const QString& infoHash)
{
    Q_EMIT torrentFinished(infoHash);
}

void TorrentStreamingService::onLibtorrentFailed(const QString& infoHash,
    const QString& reason)
{
    Q_EMIT torrentFailed(infoHash, reason);
}

} // namespace kinema::torrent
