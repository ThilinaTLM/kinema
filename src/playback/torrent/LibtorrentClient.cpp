// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/torrent/LibtorrentClient.h"

#include "config/TorrentStreamingSettings.h"
#include "core/persistence/TorrentCache.h"
#include "core/util/Magnet.h"
#include "kinema_log_torrent.h"
#include "playback/policy/MediaFileSelectionPolicy.h"

#include <KLocalizedString>

#include <QCoro/QCoroSignal>

#include <QDateTime>
#include <QFile>
#include <QFileInfo>
#include <QUuid>

#include <libtorrent/add_torrent_params.hpp>
#include <libtorrent/alert_types.hpp>
#include <libtorrent/error_code.hpp>
#include <libtorrent/file_storage.hpp>
#include <libtorrent/hex.hpp>
#include <libtorrent/magnet_uri.hpp>
#include <libtorrent/session.hpp>
#include <libtorrent/settings_pack.hpp>
#include <libtorrent/torrent_handle.hpp>
#include <libtorrent/torrent_info.hpp>
#include <libtorrent/torrent_status.hpp>

#include <stdexcept>

namespace kinema::playback::torrent {

namespace lt = libtorrent;

namespace {

constexpr int kMetadataTimeoutMs = 60'000;
constexpr int kRangeTimeoutMs    = 120'000;

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

QString shortHash(const QString& h) { return h.left(8); }

QString hashFromHandle(const lt::torrent_handle& h)
{
    if (!h.is_valid()) {
        return {};
    }
    const auto best = h.info_hashes().get_best();
    return QString::fromStdString(lt::aux::to_hex(best.to_string()))
        .toLower();
}

QString hashFromAlert(const lt::torrent_alert* a)
{
    return a ? hashFromHandle(a->handle) : QString();
}

void writeTransferSettings(const config::TorrentStreamingSettings& settings,
    lt::settings_pack& pack)
{
    const int dl = settings.maxDownloadRateKiB();
    const int ul = settings.maxUploadRateKiB();
    pack.set_int(lt::settings_pack::download_rate_limit,
        dl <= 0 ? 0 : dl * 1024);
    pack.set_int(lt::settings_pack::upload_rate_limit,
        ul <= 0 ? 0 : ul * 1024);
}

QVector<kinema::torrent::TorrentFileEntry> torrentFileEntries(
    const std::shared_ptr<const lt::torrent_info>& ti)
{
    QVector<kinema::torrent::TorrentFileEntry> files;
    if (!ti) {
        return files;
    }

    const auto& fs = ti->files();
    files.reserve(fs.num_files());
    for (int i = 0; i < fs.num_files(); ++i) {
        const lt::file_index_t idx(i);
        files.append({ i,
            QString::fromStdString(fs.file_path(idx)),
            fs.file_size(idx),
            true });
    }
    return files;
}

std::optional<domain::MediaFileEntry> requestedFileSelection(
    const QVector<kinema::torrent::TorrentFileEntry>& files,
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

} // namespace

// ---------------------------------------------------------------------------
// Session state held per (info hash, selected file).
// ---------------------------------------------------------------------------

struct LibtorrentClient::Session {
    QString             infoHash;
    QString             token;
    lt::torrent_handle  handle;
    domain::MediaFileEntry selected;
    kinema::torrent::FilePieceLayout layout;
    QString             filePath;
    QDateTime           lastActivity = QDateTime::currentDateTimeUtc();
    domain::PlaybackKey key;
    bool                keepAlive = false;
};

// ---------------------------------------------------------------------------
// Construction / lifecycle
// ---------------------------------------------------------------------------

LibtorrentClient::LibtorrentClient(
    const config::TorrentStreamingSettings& settings,
    core::TorrentCache& cache,
    QObject* parent)
    : QObject(parent)
    , m_settings(settings)
    , m_cache(cache)
{
    m_statsTimer.setInterval(2'000);
    connect(&m_statsTimer, &QTimer::timeout, this,
        &LibtorrentClient::postTorrentUpdates);

    m_idleTimer.setInterval(60'000);
    connect(&m_idleTimer, &QTimer::timeout, this,
        &LibtorrentClient::stopIdleSessions);

    qCDebug(KINEMA_TORRENT)
        << "LibtorrentClient constructed (dormant)";
}

LibtorrentClient::~LibtorrentClient()
{
    if (m_session) {
        stopAll();
    }
}

bool LibtorrentClient::ensureStarted()
{
    if (m_session) {
        return true;
    }
    qCInfo(KINEMA_TORRENT)
        << "starting libtorrent session on first use";

    lt::settings_pack pack;
    writeTransferSettings(m_settings, pack);
    // Subscribe to the alert categories the unified downloader
    // and our diagnostic logging actually consume. `status` covers
    // torrent_finished + state_changed + state_update; `error`
    // covers torrent_error / session_error; the rest give us
    // tracker / DHT / metadata visibility.
    pack.set_int(lt::settings_pack::alert_mask,
        lt::alert_category::error
            | lt::alert_category::status
            | lt::alert_category::tracker
            | lt::alert_category::dht
            | lt::alert_category::port_mapping);
    m_session = std::make_unique<lt::session>(pack);

    // Wake the GUI thread cheaply when libtorrent has alerts
    // queued. The lambda runs on libtorrent's internal thread, so
    // we hop back via a queued invoke.
    m_session->set_alert_notify([this] {
        QMetaObject::invokeMethod(this, "drainAlerts",
            Qt::QueuedConnection);
    });

    // Arm idle-stop only after we have a real session — there is
    // nothing to reap before then, and we want dormant instances to
    // stay dormant (lazy-start contract).
    m_idleTimer.start();
    return true;
}

void LibtorrentClient::applyTransferSettings()
{
    if (!m_session) {
        return;
    }
    lt::settings_pack pack;
    writeTransferSettings(m_settings, pack);
    m_session->apply_settings(pack);
}

void LibtorrentClient::setActiveHandleCount(int n)
{
    m_activeHandles = n;
    if (!m_session) {
        return;
    }
    const bool shouldRun = m_activeHandles > 0;
    if (shouldRun && !m_statsTimer.isActive()) {
        m_statsTimer.start();
    } else if (!shouldRun && m_statsTimer.isActive()) {
        m_statsTimer.stop();
    }
}

void LibtorrentClient::refreshStatsTimerRunning()
{
    setActiveHandleCount(m_sessions.size());
}

void LibtorrentClient::postTorrentUpdates()
{
    if (m_session) {
        m_session->post_torrent_updates();
    }
}

// ---------------------------------------------------------------------------
// Per-asset pipeline
// ---------------------------------------------------------------------------

QCoro::Task<PreparedSession> LibtorrentClient::prepareSession(
    const domain::Stream& stream,
    const domain::PlaybackContext& ctx,
    PrepareMode mode)
{
    ensureStarted();
    if (stream.infoHash.isEmpty()) {
        throw runtimeError(i18nc("@info:status",
            "This stream has no magnet info hash."));
    }

    const QString hash = normalizedHash(stream.infoHash);
    m_cache.markActive(hash);

    qCInfo(KINEMA_TORRENT).nospace()
        << "prepareSession[hash=" << shortHash(hash)
        << " mode="
        << (mode == PrepareMode::Background ? "background" : "streaming")
        << " release=\"" << stream.releaseName
        << "\" title=\"" << ctx.title << "\"]";

    auto it = m_sessions.find(hash);
    if (it == m_sessions.end()) {
        const QString magnet = core::magnet::build(hash, stream.releaseName);
        lt::error_code ec;
        lt::add_torrent_params atp = lt::parse_magnet_uri(
            magnet.toStdString(), ec);
        if (ec) {
            m_cache.markInactive(hash);
            qCWarning(KINEMA_TORRENT).nospace()
                << "[hash=" << shortHash(hash) << "] parse_magnet_uri: "
                << QString::fromStdString(ec.message());
            throw runtimeError(i18nc("@info:status",
                "Could not parse the magnet link: %1",
                QString::fromStdString(ec.message())));
        }
        const auto savePath = m_cache.torrentDir(hash).absolutePath();
        atp.save_path = savePath.toStdString();

        lt::torrent_handle handle = m_session->add_torrent(atp, ec);
        if (ec || !handle.is_valid()) {
            m_cache.markInactive(hash);
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

        Session state;
        state.infoHash = hash;
        state.token = QUuid::createUuid().toString(QUuid::WithoutBraces);
        state.handle = handle;
        state.key = ctx.key;
        state.lastActivity = QDateTime::currentDateTimeUtc();
        m_tokenToHash.insert(state.token, hash);
        it = m_sessions.insert(hash, std::move(state));
        refreshStatsTimerRunning();
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
                state.filePath = m_cache.torrentDir(hash)
                    .absoluteFilePath(state.selected.path);
                state.handle.clear_piece_deadlines();

                std::vector<lt::download_priority_t> priorities(
                    fs.num_files(), lt::dont_download);
                priorities[state.selected.index] = lt::top_priority;
                state.handle.prioritize_files(priorities);

                if (changedFile) {
                    m_tokenToHash.remove(state.token);
                    state.token = QUuid::createUuid().toString(
                        QUuid::WithoutBraces);
                    m_tokenToHash.insert(state.token, hash);
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
        const auto windows = kinema::torrent::startupPieceWindows(
            state.layout,
            mibToBytes(m_settings.startupBufferMiB()),
            mibToBytes(m_settings.tailBufferMiB()));
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

        const auto initial = kinema::torrent::clampRange(0,
            mibToBytes(m_settings.startupBufferMiB()), state.layout.fileSize);
        if (initial.isValid()) {
            const bool ready = co_await ensureRange(state.token, initial);
            if (!ready) {
                throw runtimeError(i18nc("@info:status",
                    "Timed out while buffering the torrent stream."));
            }
        }
    }

    m_cache.touch(hash);
    m_cache.enforceBudget();

    PreparedSession ps;
    ps.token = state.token;
    ps.fileName = fileNameForUrl(state.selected.path);
    ps.fileSize = state.layout.fileSize;
    ps.infoHash = state.infoHash;
    co_return ps;
}

QCoro::Task<bool> LibtorrentClient::ensureRange(const QString& token,
    kinema::torrent::ByteRange range)
{
    if (!m_session) {
        co_return false;
    }
    auto* state = byToken(token);
    if (!state || !range.isValid()) {
        co_return false;
    }
    state->lastActivity = QDateTime::currentDateTimeUtc();

    const auto urgent = kinema::torrent::readaheadRange(
        range.start, range.endInclusive,
        mibToBytes(m_settings.readaheadMiB()), state->layout.fileSize);
    const auto pieces = kinema::torrent::pieceRangeForBytes(
        state->layout, urgent);
    const auto requiredPieces = kinema::torrent::pieceRangeForBytes(
        state->layout, range);
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

QByteArray LibtorrentClient::readRange(const QString& token,
    kinema::torrent::ByteRange range) const
{
    if (!m_session) {
        return {};
    }
    const auto* state = byToken(token);
    if (!state || !range.isValid()) {
        return {};
    }
    QFile file(state->filePath);
    if (!file.open(QIODevice::ReadOnly) || !file.seek(range.start)) {
        return {};
    }
    return file.read(range.endInclusive - range.start + 1);
}

qint64 LibtorrentClient::fileSizeForToken(const QString& token) const
{
    const auto* state = byToken(token);
    return state ? state->layout.fileSize : 0;
}

QString LibtorrentClient::fileNameForToken(const QString& token) const
{
    const auto* state = byToken(token);
    return state ? QFileInfo(state->selected.path).fileName() : QString {};
}

void LibtorrentClient::touchToken(const QString& token)
{
    auto* state = byToken(token);
    if (!state) {
        return;
    }
    state->lastActivity = QDateTime::currentDateTimeUtc();
    m_cache.touch(state->infoHash);
}

QVector<kinema::torrent::TorrentFileEntry>
LibtorrentClient::filesForInfoHash(const QString& infoHash) const
{
    const QString h = normalizedHash(infoHash);
    const auto it = m_sessions.constFind(h);
    if (it == m_sessions.constEnd() || !it->handle.is_valid()) {
        return {};
    }
    return torrentFileEntries(it->handle.torrent_file());
}

void LibtorrentClient::setKeepAlive(const QString& infoHash, bool on)
{
    const QString h = normalizedHash(infoHash);
    auto it = m_sessions.find(h);
    if (it == m_sessions.end()) {
        return;
    }
    if (it->keepAlive == on) {
        return;
    }
    it->keepAlive = on;
    qCInfo(KINEMA_TORRENT).nospace()
        << "[hash=" << shortHash(h) << "] keepAlive=" << on;
}

void LibtorrentClient::pauseInfoHash(const QString& infoHash)
{
    const QString h = normalizedHash(infoHash);
    auto it = m_sessions.find(h);
    if (it == m_sessions.end() || !it->handle.is_valid()) {
        return;
    }
    it->handle.pause();
    qCInfo(KINEMA_TORRENT).nospace()
        << "[hash=" << shortHash(h) << "] pause (user)";
}

void LibtorrentClient::resumeInfoHash(const QString& infoHash)
{
    const QString h = normalizedHash(infoHash);
    auto it = m_sessions.find(h);
    if (it == m_sessions.end() || !it->handle.is_valid()) {
        return;
    }
    it->handle.resume();
    it->lastActivity = QDateTime::currentDateTimeUtc();
    qCInfo(KINEMA_TORRENT).nospace()
        << "[hash=" << shortHash(h) << "] resume (user)";
}

void LibtorrentClient::promoteToFull(const QString& infoHash)
{
    const QString h = normalizedHash(infoHash);
    auto it = m_sessions.find(h);
    if (it == m_sessions.end() || !it->handle.is_valid()) {
        return;
    }
    auto& state = it.value();
    // Clear every per-piece deadline so libtorrent picks pieces in
    // normal order; libtorrent's documented way is
    // `clear_piece_deadlines()` (since 1.2).
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

void LibtorrentClient::stopInfoHash(const QString& infoHash)
{
    stopHash(infoHash, "explicit");
}

void LibtorrentClient::stopForContext(const domain::PlaybackContext& ctx)
{
    if (!ctx.streamRef.infoHash.isEmpty()) {
        stopInfoHash(ctx.streamRef.infoHash);
    }
}

void LibtorrentClient::stopAll()
{
    const auto hashes = m_sessions.keys();
    for (const auto& hash : hashes) {
        stopHash(hash, "shutdown");
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

LibtorrentClient::Session* LibtorrentClient::byToken(const QString& token)
{
    const auto hash = m_tokenToHash.value(token);
    if (hash.isEmpty()) {
        return nullptr;
    }
    auto it = m_sessions.find(hash);
    return it == m_sessions.end() ? nullptr : &it.value();
}

const LibtorrentClient::Session* LibtorrentClient::byToken(
    const QString& token) const
{
    const auto hash = m_tokenToHash.value(token);
    if (hash.isEmpty()) {
        return nullptr;
    }
    auto it = m_sessions.constFind(hash);
    return it == m_sessions.constEnd() ? nullptr : &it.value();
}

void LibtorrentClient::stopHash(const QString& hash, const char* reason)
{
    const QString h = normalizedHash(hash);
    auto it = m_sessions.find(h);
    if (it == m_sessions.end()) {
        return;
    }
    qCInfo(KINEMA_TORRENT).nospace()
        << "[hash=" << shortHash(h) << "] stopping ("
        << reason << "); was keepAlive=" << it->keepAlive;
    if (it->handle.is_valid() && m_session) {
        it->handle.pause();
        m_session->remove_torrent(it->handle);
    }
    m_tokenToHash.remove(it->token);
    m_cache.markInactive(h);
    m_sessions.erase(it);
    m_cache.enforceBudget();
    refreshStatsTimerRunning();
}

void LibtorrentClient::stopIdleSessions()
{
    const auto now = QDateTime::currentDateTimeUtc();
    const auto idleSecs = m_settings.idleStopMinutes() * 60;
    QStringList stop;
    for (auto it = m_sessions.cbegin(); it != m_sessions.cend(); ++it) {
        // Pinned ("Save offline") sessions opt out of idle-stop so
        // the swarm keeps pulling pieces while the user is off doing
        // other things.
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

// ---------------------------------------------------------------------------
// Alert pump
// ---------------------------------------------------------------------------

void LibtorrentClient::drainAlerts()
{
    if (!m_session) {
        return;
    }
    std::vector<lt::alert*> alerts;
    m_session->pop_alerts(&alerts);
    for (lt::alert* a : alerts) {
        if (auto* su = lt::alert_cast<lt::state_update_alert>(a)) {
            for (const auto& st : su->status) {
                const QString hash = hashFromHandle(st.handle);
                if (hash.isEmpty()) {
                    continue;
                }
                const qint64 doneBytes = static_cast<qint64>(
                    st.total_wanted_done);
                const qint64 rate = static_cast<qint64>(
                    st.download_payload_rate);
                int eta = -1;
                if (rate > 0 && st.total_wanted > st.total_wanted_done) {
                    const qint64 remaining = static_cast<qint64>(
                        st.total_wanted - st.total_wanted_done);
                    eta = static_cast<int>(remaining / rate);
                }
                qCDebug(KINEMA_TORRENT).nospace()
                    << "[hash=" << shortHash(hash)
                    << "] peers=" << st.num_peers
                    << " seeds=" << st.num_seeds
                    << " rate=" << rate
                    << " done=" << doneBytes;
                Q_EMIT statsUpdated(hash, doneBytes, rate,
                    st.num_peers, st.num_seeds, eta, st.is_finished);
            }
            continue;
        }
        if (auto* fa = lt::alert_cast<lt::torrent_finished_alert>(a)) {
            const QString h = hashFromAlert(fa);
            qCInfo(KINEMA_TORRENT).nospace()
                << "[hash=" << shortHash(h) << "] torrent_finished";
            Q_EMIT torrentFinished(h);
            continue;
        }
        if (auto* ea = lt::alert_cast<lt::torrent_error_alert>(a)) {
            const QString h = hashFromAlert(ea);
            const QString msg = QString::fromStdString(
                ea->error.message());
            qCWarning(KINEMA_TORRENT).nospace()
                << "[hash=" << shortHash(h) << "] torrent_error: "
                << msg;
            Q_EMIT torrentFailed(h, msg);
            continue;
        }
        if (auto* ma = lt::alert_cast<lt::metadata_received_alert>(a)) {
            const QString h = hashFromAlert(ma);
            qCInfo(KINEMA_TORRENT).nospace()
                << "[hash=" << shortHash(h) << "] metadata_received";
            Q_EMIT metadataReceived(h);
            continue;
        }
        if (auto* sca = lt::alert_cast<lt::state_changed_alert>(a)) {
            qCInfo(KINEMA_TORRENT).nospace()
                << "[hash=" << shortHash(hashFromAlert(sca))
                << "] state_changed: "
                << QString::fromStdString(sca->message());
            continue;
        }
        if (auto* ata = lt::alert_cast<lt::add_torrent_alert>(a)) {
            const QString h = hashFromAlert(ata);
            if (ata->error) {
                qCWarning(KINEMA_TORRENT).nospace()
                    << "[hash=" << shortHash(h)
                    << "] add_torrent_alert error: "
                    << QString::fromStdString(ata->error.message());
            } else {
                qCDebug(KINEMA_TORRENT).nospace()
                    << "[hash=" << shortHash(h)
                    << "] add_torrent_alert ok";
            }
            continue;
        }
        if (auto* tea = lt::alert_cast<lt::tracker_error_alert>(a)) {
            qCWarning(KINEMA_TORRENT).nospace()
                << "[hash=" << shortHash(hashFromAlert(tea))
                << "] tracker_error: "
                << QString::fromStdString(tea->message());
            continue;
        }
        if (auto* twa = lt::alert_cast<lt::tracker_warning_alert>(a)) {
            qCInfo(KINEMA_TORRENT).nospace()
                << "[hash=" << shortHash(hashFromAlert(twa))
                << "] tracker_warning: "
                << QString::fromStdString(twa->message());
            continue;
        }
        if (auto* tra = lt::alert_cast<lt::tracker_reply_alert>(a)) {
            qCDebug(KINEMA_TORRENT).nospace()
                << "[hash=" << shortHash(hashFromAlert(tra))
                << "] tracker_reply: peers=" << tra->num_peers
                << " url=\"" << QString::fromStdString(tra->tracker_url())
                << "\"";
            continue;
        }
        if (lt::alert_cast<lt::dht_bootstrap_alert>(a)) {
            qCInfo(KINEMA_TORRENT) << "dht_bootstrap completed";
            continue;
        }
        if (auto* lfa = lt::alert_cast<lt::listen_failed_alert>(a)) {
            qCWarning(KINEMA_TORRENT).nospace()
                << "listen_failed: "
                << QString::fromStdString(lfa->message());
            continue;
        }
        if (a->category() & lt::alert_category::error) {
            qCWarning(KINEMA_TORRENT).noquote()
                << "alert(error):" << QString::fromStdString(a->message());
        } else {
            qCDebug(KINEMA_TORRENT).noquote()
                << "alert:" << QString::fromStdString(a->message());
        }
    }
}

} // namespace kinema::playback::torrent
