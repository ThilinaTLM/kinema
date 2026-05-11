// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QString>
#include <QStringList>
#include <QStringView>

#include <array>

namespace kinema::core::magnet {

/**
 * Default set of public trackers appended to every magnet link, so the
 * resulting URI works out of the box in any torrent client without
 * depending on DHT/PEX alone.
 *
 * NOTE: this list is intentionally identical to the one used by
 * torrentio-cli — keep them in sync if we ever rotate trackers.
 */
inline constexpr std::array<const char*, 6> kDefaultTrackers {
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://open.tracker.cl:1337/announce",
    "udp://opentracker.i2p.rocks:6969/announce",
    "udp://tracker.openbittorrent.com:6969/announce",
    "udp://exodus.desync.com:6969/announce",
    "udp://tracker.torrent.eu.org:451/announce",
};

/// Returns the default trackers as a QStringList (a convenience for tests).
QStringList defaultTrackers();

/**
 * Build a BitTorrent magnet URI.
 *
 *   magnet:?xt=urn:btih:<infoHash>&dn=<displayName>&tr=<tracker>&tr=<tracker>...
 *
 * @param infoHash     lowercase or uppercase hex info hash. Must be non-empty.
 * @param displayName  human-readable name (release title). Optional; URL-encoded.
 * @param trackers     tracker URLs to append. Defaults to kDefaultTrackers.
 */
QString build(QStringView infoHash,
    QStringView displayName = {},
    const QStringList& trackers = defaultTrackers());

} // namespace kinema::core::magnet
