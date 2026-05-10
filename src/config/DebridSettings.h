// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Debrid.h"

#include <KSharedConfig>

#include <QObject>

namespace kinema::config {

/**
 * Persisted state for the unified debrid provider knob. Both
 * Real-Debrid and AllDebrid credentials live in the system keyring
 * (via `core::TokenStore`); this class only owns the user-chosen
 * "active provider" radio plus two boolean "is a credential saved"
 * flags that the settings page uses to gate Test / Remove buttons.
 *
 * KConfig group: [Debrid]
 * Keys:
 *   provider              string ("none" / "realdebrid" / "alldebrid")
 *   realDebridConfigured  bool
 *   allDebridConfigured   bool
 *
 * The legacy [RealDebrid] group from earlier builds is intentionally
 * not migrated (pre-1.0 codebase). Users with a saved RD token will
 * see the Debrid page reflecting the keyring contents on first open
 * and can re-save / re-toggle from there.
 */
class DebridSettings : public QObject
{
    Q_OBJECT
public:
    explicit DebridSettings(KSharedConfigPtr config,
        QObject* parent = nullptr);

    api::DebridProvider activeProvider() const;
    void setActiveProvider(api::DebridProvider p);

    bool realDebridConfigured() const;
    void setRealDebridConfigured(bool);

    bool allDebridConfigured() const;
    void setAllDebridConfigured(bool);

Q_SIGNALS:
    void activeProviderChanged(api::DebridProvider);
    void realDebridConfiguredChanged(bool);
    void allDebridConfiguredChanged(bool);

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
