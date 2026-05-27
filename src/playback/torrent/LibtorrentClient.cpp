// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/torrent/LibtorrentClient.h"

#include "config/TorrentStreamingSettings.h"
#include "kinema_log_torrent.h"

#include <libtorrent/alert_types.hpp>
#include <libtorrent/hex.hpp>
#include <libtorrent/session.hpp>
#include <libtorrent/settings_pack.hpp>
#include <libtorrent/torrent_handle.hpp>
#include <libtorrent/torrent_status.hpp>

namespace kinema::playback::torrent {

namespace lt = libtorrent;

namespace {

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

QString shortHash(const QString& h) { return h.left(8); }

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

} // namespace

LibtorrentClient::LibtorrentClient(
    const config::TorrentStreamingSettings& settings, QObject* parent)
    : QObject(parent)
    , m_settings(settings)
{
    m_statsTimer.setInterval(2'000);
    connect(&m_statsTimer, &QTimer::timeout, this,
        &LibtorrentClient::postTorrentUpdates);
    qCDebug(KINEMA_TORRENT)
        << "LibtorrentClient constructed (dormant)";
}

LibtorrentClient::~LibtorrentClient() = default;

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

void LibtorrentClient::postTorrentUpdates()
{
    if (m_session) {
        m_session->post_torrent_updates();
    }
}

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
