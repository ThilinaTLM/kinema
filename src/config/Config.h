// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include "api/Types.h"
#include "core/TorrentioConfig.h"

#include <QObject>
#include <QString>

namespace kinema::config {

/**
 * Tiny KConfig-backed singleton holding preferences that Kinema wants to
 * survive across restarts. M2 only reads/writes a handful of keys; M4's
 * Settings dialog will grow this.
 *
 * Groups:
 *   [General]
 *     searchKind   = "Movie" | "Series"      (last SearchBar selection)
 *     cachedOnly   = true|false              (RD cached-only checkbox)
 *   [RealDebrid]
 *     configured   = true|false              (mirrors "is a token in the keyring?")
 *
 * Token material lives in the system keyring, never in this config.
 */
class Config : public QObject
{
    Q_OBJECT
public:
    static Config& instance();

    // Real-Debrid — whether a token is stored in the keyring.
    bool hasRealDebrid() const;
    void setHasRealDebrid(bool);

    // Cached-on-Real-Debrid-only filter.
    bool cachedOnly() const;
    void setCachedOnly(bool);

    // Last-used SearchBar mode.
    api::MediaKind searchKind() const;
    void setSearchKind(api::MediaKind);

    /// Options used to build the Torrentio URL. M2 still uses defaults
    /// (sort = Seeders, no quality/provider filters); the RD token is
    /// merged in by MainWindow from the in-memory cache. M4 will add
    /// filter UI that feeds into this.
    core::torrentio::ConfigOptions defaultTorrentioOptions() const;

Q_SIGNALS:
    void cachedOnlyChanged(bool);
    void realDebridChanged(bool);

private:
    Config();
};

} // namespace kinema::config
