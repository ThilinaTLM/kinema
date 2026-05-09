// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <KSharedConfig>

#include <QObject>

namespace kinema::config {

/**
 * Torrent-backend tuning. Kept around because the libtorrent engine
 * still consumes it directly; new code should reach for
 * `config::DownloadSettings`, which spans both the torrent and RD
 * backends. Both classes share the same `[TorrentStreaming]` KConfig
 * group so on-disk values are identical.
 */
class TorrentStreamingSettings : public QObject
{
    Q_OBJECT
public:
    explicit TorrentStreamingSettings(KSharedConfigPtr config,
        QObject* parent = nullptr);

    int cacheBudgetGb() const;
    void setCacheBudgetGb(int gb);

    int startupBufferMiB() const;
    void setStartupBufferMiB(int mib);

    int readaheadMiB() const;
    void setReadaheadMiB(int mib);

    int tailBufferMiB() const;
    void setTailBufferMiB(int mib);

    int maxDownloadRateKiB() const;
    void setMaxDownloadRateKiB(int kib);

    int maxUploadRateKiB() const;
    void setMaxUploadRateKiB(int kib);

    int idleStopMinutes() const;
    void setIdleStopMinutes(int minutes);

Q_SIGNALS:
    void cacheBudgetGbChanged(int);
    void startupBufferMiBChanged(int);
    void readaheadMiBChanged(int);
    void tailBufferMiBChanged(int);
    void maxDownloadRateKiBChanged(int);
    void maxUploadRateKiBChanged(int);
    void idleStopMinutesChanged(int);

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
