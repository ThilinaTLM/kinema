// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "services/StreamActions.h"

#include "core/io/OpenUrl.h"
#include "kinema_log_ui.h"
#include "torrent/Magnet.h"

#include <QClipboard>
#include <QGuiApplication>

#include <KLocalizedString>

namespace kinema::services {

namespace {

QString clipboardCopyMessage(bool isMagnet)
{
    return isMagnet ? i18nc("@info:status", "Magnet link copied to clipboard")
                    : i18nc("@info:status", "Direct URL copied to clipboard");
}

} // namespace

StreamActions::StreamActions(QObject* parent) : QObject(parent) { }

void StreamActions::launchOpenUrlJob(const QUrl& url,
                                     const QString& successMsg,
                                     const QString& failurePrefix,
                                     const char* failureLogTag)
{
    const QString successCopy = successMsg;
    const QString failureCopy = failurePrefix;
    const char* tag = failureLogTag;
    core::io::openExternal(
        url, this, [this, successCopy, failureCopy, tag](const core::io::OpenExternalResult& r) {
            if (!r.ok) {
                Q_EMIT statusMessage(i18nc("@info:status", "%1: %2", failureCopy, r.errorString),
                                     6000);
                qCWarning(KINEMA_UI) << tag << "failed:" << r.errorString;
                return;
            }
            Q_EMIT statusMessage(successCopy, 3000);
        });
}

void StreamActions::copyMagnet(const domain::Stream& stream)
{
    if (stream.infoHash.isEmpty()) {
        return;
    }
    QGuiApplication::clipboard()->setText(
        kinema::torrent::magnet::build(stream.infoHash, stream.releaseName));
    Q_EMIT statusMessage(clipboardCopyMessage(true), 3000);
}

void StreamActions::openMagnet(const domain::Stream& stream)
{
    if (stream.infoHash.isEmpty()) {
        return;
    }
    const auto magnet = kinema::torrent::magnet::build(stream.infoHash, stream.releaseName);
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
                     i18nc("@info:status", "Opening stream…"),
                     i18nc("@info:status", "Could not open URL"),
                     "OpenUrlJob (direct)");
}

void StreamActions::copyReleaseName(const domain::Stream& stream)
{
    const QString text = !stream.releaseName.isEmpty() ? stream.releaseName : stream.qualityLabel;
    if (text.isEmpty()) {
        return;
    }
    QGuiApplication::clipboard()->setText(text);
    Q_EMIT statusMessage(i18nc("@info:status", "Release name copied to clipboard"), 3000);
}

} // namespace kinema::services
