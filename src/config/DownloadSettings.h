// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <KSharedConfig>

#include <QObject>

namespace kinema::config {

/**
 * User-tunable behavior for the unified downloader (the torrent
 * backend and the Real-Debrid HTTP backend).
 *
 * KConfig keys are kept in the existing `[TorrentStreaming]` group
 * \u2014 these knobs are not torrent-specific anymore but the on-disk
 * config layout is preserved per AGENTS.md (no migrations).
 */
class DownloadSettings : public QObject
{
    Q_OBJECT
public:
    explicit DownloadSettings(KSharedConfigPtr config,
        QObject* parent = nullptr);

    /// Disk budget for the ephemeral playback cache, in GB. Pinned
    /// explicit downloads do NOT count against this budget.
    int cacheBudgetGb() const;
    void setCacheBudgetGb(int gb);

    /// Bytes to fully buffer at the head of a stream before playback
    /// begins.
    int startupBufferMiB() const;
    void setStartupBufferMiB(int mib);

    /// Extra bytes to keep ready ahead of the current read position.
    int readaheadMiB() const;
    void setReadaheadMiB(int mib);

    /// Bytes to fully buffer at the tail of a stream so the player can
    /// read trailing index data without waiting.
    int tailBufferMiB() const;
    void setTailBufferMiB(int mib);

    /// Per-asset download rate cap in KiB/s. 0 = unlimited.
    int maxDownloadRateKiB() const;
    void setMaxDownloadRateKiB(int kib);

    /// Torrent-specific upload rate cap in KiB/s.
    int maxUploadRateKiB() const;
    void setMaxUploadRateKiB(int kib);

    /// How long (in minutes) an idle asset session stays loaded
    /// before the manager tears it down to release sockets / threads.
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
