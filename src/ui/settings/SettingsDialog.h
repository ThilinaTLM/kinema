// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include <KPageDialog>

namespace kinema::core {
class HttpClient;
class TokenStore;
}

namespace kinema::ui::settings {

class GeneralSettingsPage;
class FiltersSettingsPage;
class PlayerSettingsPage;
class RealDebridSettingsPage;
class TmdbSettingsPage;

/**
 * Top-level Settings dialog. Five pages:
 *
 *   General — default search kind + Torrentio sort.
 *   Filters — server-side resolution/variant exclusions + client-side
 *             keyword blocklist.
 *   Player  — mpv / VLC / custom command.
 *   Real-Debrid — token management (async, self-contained; not part of
 *                 the Apply/OK flow).
 *   TMDB (Discover) — optional override of the bundled TMDB token.
 *
 * On Apply / OK each page's apply() is called. Real-Debrid's
 * tokenChanged and TMDB's tmdbTokenChanged signals are forwarded
 * through the dialog so MainWindow can keep its in-memory tokens
 * fresh without a keyring round-trip.
 */
class SettingsDialog : public KPageDialog
{
    Q_OBJECT
public:
    SettingsDialog(core::HttpClient* http,
        core::TokenStore* tokens,
        QWidget* parent = nullptr);

Q_SIGNALS:
    /// Forwarded from RealDebridSettingsPage. Empty string = removed.
    void tokenChanged(const QString& token);
    /// Forwarded from TmdbSettingsPage. Empty string means the user
    /// removed their override — the effective token falls back to the
    /// compile-time default (if any).
    void tmdbTokenChanged(const QString& token);

private:
    void applyAll();
    void resetAllToDefaults();

    GeneralSettingsPage* m_generalPage {};
    FiltersSettingsPage* m_filtersPage {};
    PlayerSettingsPage* m_playerPage {};
    RealDebridSettingsPage* m_rdPage {};
    TmdbSettingsPage* m_tmdbPage {};
};

} // namespace kinema::ui::settings
