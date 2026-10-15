// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "core/io/HttpError.h"
#include "domain/Media.h"
#include "torrent/TorrentFileEntry.h"

#include <KLocalizedString>

#include <QCoro/QCoroSignal>
#include <QCoro/QCoroTask>
#include <QList>
#include <QTimer>
#include <QVector>

namespace kinema::playback::sources {

/// Sleep for `ms` without blocking the event loop (shared by the
/// debrid resolvers' polling loops).
inline QCoro::Task<void> debridSleep(int ms)
{
    QTimer t;
    t.setSingleShot(true);
    t.start(ms);
    co_await qCoro(&t, &QTimer::timeout);
}

/// Guard that a resolution call has an actionable info hash.
inline void requireInfoHash(const domain::AssetRef& ref,
    const QString& providerName)
{
    if (ref.infoHash.isEmpty()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("%1 resolution requires an info hash.", providerName));
    }
}

/// Tracks elapsed time across a provider polling loop and throws
/// nothing itself; callers check `expired()` after each iteration
/// and report their own timeout error. `tick()` sleeps one interval
/// and advances the budget.
class PollBudget
{
public:
    explicit PollBudget(int timeoutMs, int intervalMs = 1'000)
        : m_timeoutMs(timeoutMs)
        , m_intervalMs(intervalMs)
    {
    }

    bool expired() const noexcept { return m_elapsedMs >= m_timeoutMs; }

    QCoro::Task<void> tick() noexcept
    {
        co_await debridSleep(m_intervalMs);
        m_elapsedMs += m_intervalMs;
    }

private:
    int m_timeoutMs = 0;
    int m_intervalMs = 1'000;
    int m_elapsedMs = 0;
};

/// Flatten a provider file list into 0-based positional
/// `TorrentFileEntry`s. Both debrid providers expose `.path` and
/// `.bytes`, so a single template covers them; this normalises
/// provider-specific id conventions for series auto-next.
template <typename FileT>
QVector<torrent::TorrentFileEntry> flattenFileList(const QList<FileT>& files)
{
    QVector<torrent::TorrentFileEntry> out;
    out.reserve(files.size());
    for (int i = 0; i < files.size(); ++i) {
        out.append(torrent::TorrentFileEntry { i, files[i].path, files[i].bytes });
    }
    return out;
}

} // namespace kinema::playback::sources
