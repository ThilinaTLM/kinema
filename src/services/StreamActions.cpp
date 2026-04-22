// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "services/StreamActions.h"

#include "controllers/HistoryController.h"
#include "core/Magnet.h"
#include "core/PlayerLauncher.h"
#include "kinema_debug.h"

#include <KIO/OpenUrlJob>
#include <KLocalizedString>

#include <QClipboard>
#include <QGuiApplication>
#include <QUrl>

namespace kinema::services {

StreamActions::StreamActions(core::PlayerLauncher* launcher, QObject* parent)
    : QObject(parent)
    , m_launcher(launcher)
{
}

void StreamActions::setHistoryController(
    controllers::HistoryController* history)
{
    m_history = history;
}

void StreamActions::copyMagnet(const api::Stream& stream)
{
    if (stream.infoHash.isEmpty()) {
        return;
    }
    const auto magnet = core::magnet::build(
        stream.infoHash, stream.releaseName);
    QGuiApplication::clipboard()->setText(magnet);
    Q_EMIT statusMessage(
        i18nc("@info:status", "Magnet link copied to clipboard"), 3000);
}

void StreamActions::openMagnet(const api::Stream& stream)
{
    if (stream.infoHash.isEmpty()) {
        return;
    }
    const auto magnet = core::magnet::build(
        stream.infoHash, stream.releaseName);
    auto* job = new KIO::OpenUrlJob(QUrl(magnet), this);
    job->setRunExecutables(false);
    connect(job, &KJob::result, this, [this, job] {
        if (job->error()) {
            Q_EMIT statusMessage(
                i18nc("@info:status", "Could not open magnet: %1",
                    job->errorString()),
                6000);
            qCWarning(KINEMA) << "OpenUrlJob failed:"
                              << job->errorString();
        } else {
            Q_EMIT statusMessage(
                i18nc("@info:status",
                    "Magnet sent to default handler"),
                3000);
        }
    });
    job->start();
}

void StreamActions::copyDirectUrl(const api::Stream& stream)
{
    if (stream.directUrl.isEmpty()) {
        return;
    }
    QGuiApplication::clipboard()->setText(stream.directUrl.toString());
    Q_EMIT statusMessage(
        i18nc("@info:status", "Direct URL copied to clipboard"), 3000);
}

void StreamActions::openDirectUrl(const api::Stream& stream)
{
    if (stream.directUrl.isEmpty()) {
        return;
    }
    auto* job = new KIO::OpenUrlJob(stream.directUrl, this);
    job->setRunExecutables(false);
    connect(job, &KJob::result, this, [this, job] {
        if (job->error()) {
            Q_EMIT statusMessage(
                i18nc("@info:status", "Could not open URL: %1",
                    job->errorString()),
                6000);
            qCWarning(KINEMA) << "OpenUrlJob (direct) failed:"
                              << job->errorString();
        } else {
            Q_EMIT statusMessage(
                i18nc("@info:status", "Opening stream\u2026"), 3000);
        }
    });
    job->start();
}

void StreamActions::play(const api::Stream& stream,
    const api::PlaybackContext& ctxIn)
{
    if (stream.directUrl.isEmpty()) {
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "This release has no direct URL \u2014 "
                "use Real-Debrid for one-click play."),
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

    m_launcher->play(stream.directUrl, ctx);
}

} // namespace kinema::services
