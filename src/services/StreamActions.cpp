// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "services/StreamActions.h"

#include "controllers/HistoryController.h"
#include "core/Magnet.h"
#include "core/PlayerLauncher.h"
#include "core/HttpErrorPresenter.h"
#include "download/DownloadManager.h"
#include "kinema_log_app.h"
#include "torrent/TorrentStreamingService.h"

#include <KIO/OpenUrlJob>
#include <KLocalizedString>

#include <QClipboard>
#include <QGuiApplication>
#include <QUrl>

namespace kinema::services {

namespace {

QString clipboardCopyMessage(bool isMagnet)
{
    return isMagnet
        ? i18nc("@info:status", "Magnet link copied to clipboard")
        : i18nc("@info:status", "Direct URL copied to clipboard");
}

} // namespace

StreamActions::StreamActions(core::PlayerLauncher* launcher,
    torrent::TorrentStreamingService* torrentStreaming, QObject* parent)
    : QObject(parent)
    , m_launcher(launcher)
    , m_torrentStreaming(torrentStreaming)
{
}

void StreamActions::launchOpenUrlJob(const QUrl& url,
    const QString& successMsg, const QString& failurePrefix,
    const char* failureLogTag)
{
    auto* job = new KIO::OpenUrlJob(url, this);
    job->setRunExecutables(false);
    connect(job, &KJob::result, this, [this, job, successMsg,
                                          failurePrefix, failureLogTag] {
        if (job->error()) {
            Q_EMIT statusMessage(
                i18nc("@info:status", "%1: %2",
                    failurePrefix, job->errorString()),
                6000);
            qCWarning(KINEMA_APP) << failureLogTag << "failed:"
                              << job->errorString();
            return;
        }
        Q_EMIT statusMessage(successMsg, 3000);
    });
    job->start();
}

void StreamActions::setHistoryController(
    controllers::HistoryController* history)
{
    m_history = history;
}

void StreamActions::setDownloadManager(download::DownloadManager* manager)
{
    m_downloadManager = manager;
}

void StreamActions::copyMagnet(const api::Stream& stream)
{
    if (stream.infoHash.isEmpty()) {
        return;
    }
    QGuiApplication::clipboard()->setText(
        core::magnet::build(stream.infoHash, stream.releaseName));
    Q_EMIT statusMessage(clipboardCopyMessage(true), 3000);
}

void StreamActions::openMagnet(const api::Stream& stream)
{
    if (stream.infoHash.isEmpty()) {
        return;
    }
    const auto magnet = core::magnet::build(
        stream.infoHash, stream.releaseName);
    launchOpenUrlJob(QUrl(magnet),
        i18nc("@info:status", "Magnet sent to default handler"),
        i18nc("@info:status", "Could not open magnet"),
        "OpenUrlJob (magnet)");
}

void StreamActions::copyDirectUrl(const api::Stream& stream)
{
    if (stream.directUrl.isEmpty()) {
        return;
    }
    QGuiApplication::clipboard()->setText(stream.directUrl.toString());
    Q_EMIT statusMessage(clipboardCopyMessage(false), 3000);
}

void StreamActions::openDirectUrl(const api::Stream& stream)
{
    if (stream.directUrl.isEmpty()) {
        return;
    }
    launchOpenUrlJob(stream.directUrl,
        i18nc("@info:status", "Opening stream\u2026"),
        i18nc("@info:status", "Could not open URL"),
        "OpenUrlJob (direct)");
}

void StreamActions::play(const api::Stream& stream,
    const api::PlaybackContext& ctxIn)
{
    if (stream.directUrl.isEmpty() && stream.infoHash.isEmpty()) {
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "This release has no playable URL or magnet."),
            5000);
        return;
    }

    // Build a local copy so we can fill in the fields the caller
    // can't know (streamRef and resumeSeconds).
    api::PlaybackContext ctx = ctxIn;
    ctx.streamRef = api::HistoryStreamRef::fromStream(stream);

    // Fall back to the stream's release name for display when the
    // caller didn't set a title (e.g. legacy paths that haven't been
    // updated to thread identity through).
    if (ctx.title.isEmpty()) {
        ctx.title = stream.releaseName.isEmpty()
            ? stream.qualityLabel
            : stream.releaseName;
    }

    if (m_history) {
        ctx.resumeSeconds = m_history->resumeSecondsFor(ctx.key);
        m_history->onPlayStarting(ctx);
    }

    // Preferred path: every backend serves through the unified
    // downloader so the player only ever sees localhost URLs.
    if (m_downloadManager && !stream.infoHash.isEmpty()) {
        const auto epoch = ++m_playEpoch;
        auto task = playLocalTask(stream, ctx, epoch);
        Q_UNUSED(task);
        return;
    }

    // Direct-URL-only candidate (no info hash) — fall back to the
    // remote URL until discovery learns to recover an info hash.
    if (!stream.directUrl.isEmpty()) {
        ++m_playEpoch;
        m_launcher->play(stream.directUrl, ctx);
        return;
    }

    // Legacy fallback. Only reached when the unified downloader is
    // not wired (some unit-test setups). Production always takes the
    // `m_downloadManager` branch above.
    if (!m_torrentStreaming) {
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "Torrent streaming is not available in this build."),
            5000);
        return;
    }

    const auto epoch = ++m_playEpoch;
    auto task = playTorrentTask(stream, ctx, epoch);
    Q_UNUSED(task);
}

QCoro::Task<void> StreamActions::playLocalTask(api::Stream stream,
    api::PlaybackContext ctx, quint64 epoch)
{
    try {
        const QUrl url = co_await m_downloadManager->prepareForPlayback(
            stream, ctx);
        if (epoch != m_playEpoch) {
            co_return;
        }
        m_launcher->play(url, ctx);
    } catch (const std::exception& e) {
        if (epoch != m_playEpoch) {
            co_return;
        }
        Q_EMIT statusMessage(core::describeError(e, "local playback"),
            6000);
    }
}

QCoro::Task<void> StreamActions::playTorrentTask(api::Stream stream,
    api::PlaybackContext ctx, quint64 epoch)
{
    try {
        const QUrl url = co_await m_torrentStreaming->prepare(stream, ctx);
        if (epoch != m_playEpoch) {
            co_return;
        }
        m_launcher->play(url, ctx);
    } catch (const std::exception& e) {
        if (epoch != m_playEpoch) {
            co_return;
        }
        Q_EMIT statusMessage(core::describeError(e, "torrent playback"),
            6000);
    }
}

} // namespace kinema::services
