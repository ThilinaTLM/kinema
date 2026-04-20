// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include "api/Types.h"
#include "core/Player.h"
#include "core/TorrentioConfig.h"

#include <QByteArray>
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
 *   [Player]
 *     preferred    = "mpv" | "vlc" | "custom"   (Play action target)
 *     customCommand= <free-form command line>  (only for preferred="custom")
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

    // Series focus view's vertical splitter state (QSplitter::saveState).
    // Empty on first run — SeriesFocusView falls back to 40/60 default.
    QByteArray focusSplitterState() const;
    void setFocusSplitterState(QByteArray state);

    // Preferred external media player for the Play action. Defaults to
    // mpv. The launcher falls back to the other known players if the
    // preferred one isn't on $PATH.
    core::player::Kind preferredPlayer() const;
    void setPreferredPlayer(core::player::Kind);

    // Free-form command line used when preferredPlayer() == Custom.
    // May contain the literal token "{url}" where the URL should be
    // substituted; otherwise the URL is appended as a final argument.
    QString customPlayerCommand() const;
    void setCustomPlayerCommand(const QString& command);

    /// Options used to build the Torrentio URL. M2 still uses defaults
    /// (sort = Seeders, no quality/provider filters); the RD token is
    /// merged in by MainWindow from the in-memory cache. M4 will add
    /// filter UI that feeds into this.
    core::torrentio::ConfigOptions defaultTorrentioOptions() const;

Q_SIGNALS:
    void cachedOnlyChanged(bool);
    void realDebridChanged(bool);
    void preferredPlayerChanged(core::player::Kind);

private:
    Config();
};

} // namespace kinema::config
