// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "config/TorrentStreamingSettings.h"

#include "config/ConfigAccess.h"

namespace kinema::config {

namespace {
constexpr auto kGroup = "TorrentStreaming";
constexpr auto kCacheBudgetGb = "cacheBudgetGb";
constexpr auto kStartupBufferMiB = "startupBufferMiB";
constexpr auto kReadaheadMiB = "readaheadMiB";
constexpr auto kTailBufferMiB = "tailBufferMiB";
constexpr auto kMaxDownloadRateKiB = "maxDownloadRateKiB";
constexpr auto kMaxUploadRateKiB = "maxUploadRateKiB";
constexpr auto kIdleStopMinutes = "idleStopMinutes";

constexpr int kDefaultCacheBudgetGb = 50;
constexpr int kDefaultStartupBufferMiB = 32;
constexpr int kDefaultReadaheadMiB = 256;
constexpr int kDefaultTailBufferMiB = 16;
constexpr int kDefaultDownloadRateKiB = 0;
constexpr int kDefaultUploadRateKiB = 512;
constexpr int kDefaultIdleStopMinutes = 5;
}

TorrentStreamingSettings::TorrentStreamingSettings(
    KSharedConfigPtr config, QObject* parent)
    : QObject(parent)
    , m_config(std::move(config))
{
}

int TorrentStreamingSettings::cacheBudgetGb() const
{
    return qBound(1, detail::read(m_config, kGroup, kCacheBudgetGb,
                         kDefaultCacheBudgetGb), 1024);
}

void TorrentStreamingSettings::setCacheBudgetGb(int gb)
{
    gb = qBound(1, gb, 1024);
    if (cacheBudgetGb() == gb) {
        return;
    }
    detail::write(m_config, kGroup, kCacheBudgetGb, gb);
    Q_EMIT cacheBudgetGbChanged(gb);
}

int TorrentStreamingSettings::startupBufferMiB() const
{
    return qBound(1, detail::read(m_config, kGroup, kStartupBufferMiB,
                         kDefaultStartupBufferMiB), 4096);
}

void TorrentStreamingSettings::setStartupBufferMiB(int mib)
{
    mib = qBound(1, mib, 4096);
    if (startupBufferMiB() == mib) {
        return;
    }
    detail::write(m_config, kGroup, kStartupBufferMiB, mib);
    Q_EMIT startupBufferMiBChanged(mib);
}

int TorrentStreamingSettings::readaheadMiB() const
{
    return qBound(1, detail::read(m_config, kGroup, kReadaheadMiB,
                         kDefaultReadaheadMiB), 8192);
}

void TorrentStreamingSettings::setReadaheadMiB(int mib)
{
    mib = qBound(1, mib, 8192);
    if (readaheadMiB() == mib) {
        return;
    }
    detail::write(m_config, kGroup, kReadaheadMiB, mib);
    Q_EMIT readaheadMiBChanged(mib);
}

int TorrentStreamingSettings::tailBufferMiB() const
{
    return qBound(0, detail::read(m_config, kGroup, kTailBufferMiB,
                         kDefaultTailBufferMiB), 1024);
}

void TorrentStreamingSettings::setTailBufferMiB(int mib)
{
    mib = qBound(0, mib, 1024);
    if (tailBufferMiB() == mib) {
        return;
    }
    detail::write(m_config, kGroup, kTailBufferMiB, mib);
    Q_EMIT tailBufferMiBChanged(mib);
}

int TorrentStreamingSettings::maxDownloadRateKiB() const
{
    return qMax(0, detail::read(m_config, kGroup, kMaxDownloadRateKiB,
                       kDefaultDownloadRateKiB));
}

void TorrentStreamingSettings::setMaxDownloadRateKiB(int kib)
{
    kib = qMax(0, kib);
    if (maxDownloadRateKiB() == kib) {
        return;
    }
    detail::write(m_config, kGroup, kMaxDownloadRateKiB, kib);
    Q_EMIT maxDownloadRateKiBChanged(kib);
}

int TorrentStreamingSettings::maxUploadRateKiB() const
{
    return qMax(0, detail::read(m_config, kGroup, kMaxUploadRateKiB,
                       kDefaultUploadRateKiB));
}

void TorrentStreamingSettings::setMaxUploadRateKiB(int kib)
{
    kib = qMax(0, kib);
    if (maxUploadRateKiB() == kib) {
        return;
    }
    detail::write(m_config, kGroup, kMaxUploadRateKiB, kib);
    Q_EMIT maxUploadRateKiBChanged(kib);
}

int TorrentStreamingSettings::idleStopMinutes() const
{
    return qBound(1, detail::read(m_config, kGroup, kIdleStopMinutes,
                         kDefaultIdleStopMinutes), 240);
}

void TorrentStreamingSettings::setIdleStopMinutes(int minutes)
{
    minutes = qBound(1, minutes, 240);
    if (idleStopMinutes() == minutes) {
        return;
    }
    detail::write(m_config, kGroup, kIdleStopMinutes, minutes);
    Q_EMIT idleStopMinutesChanged(minutes);
}

} // namespace kinema::config
