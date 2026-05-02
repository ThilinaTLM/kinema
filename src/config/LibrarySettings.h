// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <KSharedConfig>

#include <QObject>

namespace kinema::config {

/**
 * Library page persisted view state. Write-through, no signals — the
 * Library view-model owns the cached value and emits its own
 * notifications.
 *
 * KConfig group: [Library]
 * Keys:
 *   smartMode    bool, defaults true (Smart rails view on first run)
 */
class LibrarySettings : public QObject
{
    Q_OBJECT
public:
    explicit LibrarySettings(KSharedConfigPtr config,
        QObject* parent = nullptr);

    bool smartMode() const;
    void setSmartMode(bool);

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
