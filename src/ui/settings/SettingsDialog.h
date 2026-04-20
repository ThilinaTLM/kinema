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

/**
 * Top-level Settings dialog. Four pages:
 *
 *   General — default search kind + Torrentio sort.
 *   Filters — server-side resolution/variant exclusions + client-side
 *             keyword blocklist.
 *   Player  — mpv / VLC / custom command.
 *   Real-Debrid — token management (async, self-contained; not part of
 *                 the Apply/OK flow).
 *
 * On Apply / OK each page's apply() is called. Real-Debrid's
 * tokenChanged signal is forwarded through the dialog so MainWindow
 * can keep its in-memory token fresh without a keyring round-trip.
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

private:
    void applyAll();
    void resetAllToDefaults();

    GeneralSettingsPage* m_generalPage {};
    FiltersSettingsPage* m_filtersPage {};
    PlayerSettingsPage* m_playerPage {};
    RealDebridSettingsPage* m_rdPage {};
};

} // namespace kinema::ui::settings
