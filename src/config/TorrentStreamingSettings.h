// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <KSharedConfig>

#include <QObject>

namespace kinema::config {

/**
 * User-tunable behavior for the unified downloader (the torrent
 * backend and the Real-Debrid HTTP backend), plus the libtorrent
 * engine's streaming knobs.
 *
 * KConfig keys live in the `[TorrentStreaming]` group — these
 * knobs are not torrent-specific anymore but the on-disk config
 * layout is preserved per AGENTS.md (no migrations).
 */
class TorrentStreamingSettings : public QObject
{
    Q_OBJECT
public:
    explicit TorrentStreamingSettings(KSharedConfigPtr config,
        QObject* parent = nullptr);

    /// Disk budget for the ephemeral playback cache, in GB. Pinned
    /// explicit downloads do NOT count against this budget.
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

    /// Maximum number of background prefetch jobs that may run in
    /// parallel. Foreground `ensureRange()` requests always preempt.
    int maxBackgroundJobs() const;
    void setMaxBackgroundJobs(int count);

Q_SIGNALS:
    void cacheBudgetGbChanged(int);
    void startupBufferMiBChanged(int);
    void readaheadMiBChanged(int);
    void tailBufferMiBChanged(int);
    void maxDownloadRateKiBChanged(int);
    void maxUploadRateKiBChanged(int);
    void idleStopMinutesChanged(int);
    void maxBackgroundJobsChanged(int);

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
