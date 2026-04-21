// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <KSharedConfig>

#include <QObject>

namespace kinema::config {

/**
 * Real-Debrid "is a token stored in the keyring" bit. The actual token
 * material lives in the system keyring via core::TokenStore; this
 * class only tracks the persistent flag used to light up RD UI.
 *
 * KConfig group: [RealDebrid]
 * Keys:
 *   configured   bool
 */
class RealDebridSettings : public QObject
{
    Q_OBJECT
public:
    explicit RealDebridSettings(KSharedConfigPtr config,
        QObject* parent = nullptr);

    bool configured() const;
    void setConfigured(bool);

Q_SIGNALS:
    void configuredChanged(bool);

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
