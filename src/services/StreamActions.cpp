// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "services/StreamActions.h"

#include "controllers/HistoryController.h"
#include "core/util/Magnet.h"
#include "core/mpv/PlayerLauncher.h"
#include "core/io/HttpErrorPresenter.h"
#include "core/io/OpenUrl.h"
#include "download/DownloadManager.h"
#include "kinema_log_ui.h"
#include "torrent/TorrentStreamingService.h"

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
    const QString successCopy = successMsg;
    const QString failureCopy = failurePrefix;
    const char* tag = failureLogTag;
    core::io::openExternal(url, this,
        [this, successCopy, failureCopy, tag](
            const core::io::OpenExternalResult& r) {
            if (!r.ok) {
                Q_EMIT statusMessage(
                    i18nc("@info:status", "%1: %2",
                        failureCopy, r.errorString),
                    6000);
                qCWarning(KINEMA_UI) << tag << "failed:" << r.errorString;
                return;
            }
            Q_EMIT statusMessage(successCopy, 3000);
        });
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

void StreamActions::copyMagnet(const domain::Stream& stream)
{
    if (stream.infoHash.isEmpty()) {
        return;
    }
    QGuiApplication::clipboard()->setText(
        core::magnet::build(stream.infoHash, stream.releaseName));
    Q_EMIT statusMessage(clipboardCopyMessage(true), 3000);
}

void StreamActions::openMagnet(const domain::Stream& stream)
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

void StreamActions::copyDirectUrl(const domain::Stream& stream)
{
    if (stream.directUrl.isEmpty()) {
        return;
    }
    QGuiApplication::clipboard()->setText(stream.directUrl.toString());
    Q_EMIT statusMessage(clipboardCopyMessage(false), 3000);
}

void StreamActions::openDirectUrl(const domain::Stream& stream)
{
    if (stream.directUrl.isEmpty()) {
        return;
    }
    launchOpenUrlJob(stream.directUrl,
        i18nc("@info:status", "Opening stream\u2026"),
        i18nc("@info:status", "Could not open URL"),
        "OpenUrlJob (direct)");
}

void StreamActions::copyReleaseName(const domain::Stream& stream)
{
    // Prefer the full release name; fall back to a short label so
    // releases that only carry a quality tag still copy something
    // meaningful instead of silently no-op'ing.
    const QString text = !stream.releaseName.isEmpty()
        ? stream.releaseName
        : stream.qualityLabel;
    if (text.isEmpty()) {
        return;
    }
    QGuiApplication::clipboard()->setText(text);
    Q_EMIT statusMessage(
        i18nc("@info:status", "Release name copied to clipboard"),
        3000);
}

void StreamActions::play(const domain::Stream& stream,
    const domain::PlaybackContext& ctxIn)
{
    playInternal(stream, ctxIn, std::nullopt);
}

void StreamActions::playWithBackend(const domain::Stream& stream,
    const domain::PlaybackContext& ctxIn,
    domain::DownloadBackendKind backend)
{
    playInternal(stream, ctxIn, backend);
}

void StreamActions::playInternal(const domain::Stream& stream,
    const domain::PlaybackContext& ctxIn,
    std::optional<domain::DownloadBackendKind> backendOverride)
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
    domain::PlaybackContext ctx = ctxIn;
    ctx.streamRef = domain::HistoryStreamRef::fromStream(stream);

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
        auto task = playLocalTask(stream, ctx, epoch, backendOverride);
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

void StreamActions::download(const domain::Stream& stream,
    const domain::PlaybackContext& ctxIn)
{
    if (!m_downloadManager) {
        Q_EMIT statusMessage(i18nc("@info:status",
            "Downloads are not available in this build."), 5000);
        return;
    }
    domain::PlaybackContext ctx = ctxIn;
    ctx.streamRef = domain::HistoryStreamRef::fromStream(stream);
    if (ctx.title.isEmpty()) {
        ctx.title = stream.releaseName.isEmpty()
            ? stream.qualityLabel
            : stream.releaseName;
    }
    m_downloadManager->enqueueDownload(stream, ctx);
}

void StreamActions::downloadWithBackend(const domain::Stream& stream,
    const domain::PlaybackContext& ctxIn,
    domain::DownloadBackendKind backend)
{
    if (!m_downloadManager) {
        Q_EMIT statusMessage(i18nc("@info:status",
            "Downloads are not available in this build."), 5000);
        return;
    }
    domain::PlaybackContext ctx = ctxIn;
    ctx.streamRef = domain::HistoryStreamRef::fromStream(stream);
    if (ctx.title.isEmpty()) {
        ctx.title = stream.releaseName.isEmpty()
            ? stream.qualityLabel
            : stream.releaseName;
    }
    m_downloadManager->enqueueDownload(stream, ctx, backend);
}

QCoro::Task<void> StreamActions::playLocalTask(domain::Stream stream,
    domain::PlaybackContext ctx, quint64 epoch,
    std::optional<domain::DownloadBackendKind> backendOverride)
{
    try {
        const QUrl url = co_await m_downloadManager->prepareForPlayback(
            stream, ctx, backendOverride);
        if (epoch != m_playEpoch) {
            co_return;
        }
        // attachPlayer was already called by `prepareForPlayback`
        // so the Downloads view sees a `Streaming`/`Downloading +
        // Playing` chip immediately. detachPlayer is currently a
        // gap: external player processes are launched detached and
        // we don't watch their lifetime, so OnDemand sessions
        // remain `Active` until the engine's idle-stop timer
        // expires (TorrentStreamingSettings::idleStopMinutes).
        // Embedded player attach/detach can be wired here later.
        m_launcher->play(url, ctx);
    } catch (const std::exception& e) {
        if (epoch != m_playEpoch) {
            co_return;
        }
        Q_EMIT statusMessage(core::describeError(e, "local playback"),
            6000);
    }
}

QCoro::Task<void> StreamActions::playTorrentTask(domain::Stream stream,
    domain::PlaybackContext ctx, quint64 epoch)
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
