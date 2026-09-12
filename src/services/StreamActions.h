// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Media.h"

#include <QObject>
#include <QUrl>

namespace kinema::services {

/**
 * Utility-only actions for a stream row: clipboard and external URL
 * dispatch. Playback and download orchestration lives in
 * playback::session::PlaybackSessionManager / DownloadController.
 */
class StreamActions : public QObject
{
    Q_OBJECT
public:
    explicit StreamActions(QObject* parent = nullptr);

public Q_SLOTS:
    void copyMagnet(const domain::Stream& stream);
    void openMagnet(const domain::Stream& stream);
    void copyDirectUrl(const domain::Stream& stream);
    void openDirectUrl(const domain::Stream& stream);
    void copyReleaseName(const domain::Stream& stream);

Q_SIGNALS:
    void statusMessage(const QString& text, int timeoutMs = 3000);

private:
    void launchOpenUrlJob(const QUrl& url,
                          const QString& successMsg,
                          const QString& failurePrefix,
                          const char* failureLogTag);
};

} // namespace kinema::services
