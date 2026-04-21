// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QWidget>

#include <QCoro/QCoroTask>

class QLabel;
class QLineEdit;
class QPushButton;

namespace kinema::core {
class HttpClient;
class TokenStore;
}

namespace kinema::config {
class RealDebridSettings;
}

namespace kinema::ui::settings {

/**
 * Settings page for Real-Debrid integration.
 *
 * Self-contained: has its own inline "Test connection", "Save", and
 * "Remove" buttons because the token operations are async against the
 * system keyring / RD API and don't fit into the owning SettingsDialog's
 * synchronous Apply flow.
 *
 * Emits tokenChanged(token) on a successful Save (or Remove, with an
 * empty string). MainWindow listens to keep its in-memory m_rdToken
 * in sync without a keyring round-trip.
 */
class RealDebridSettingsPage : public QWidget
{
    Q_OBJECT
public:
    RealDebridSettingsPage(core::HttpClient* http,
        core::TokenStore* tokens,
        config::RealDebridSettings& rdSettings,
        QWidget* parent = nullptr);

Q_SIGNALS:
    /// Fired after a successful Save (non-empty token) or Remove
    /// (empty string).
    void tokenChanged(const QString& token);

private:
    QCoro::Task<void> loadExistingToken();
    QCoro::Task<void> testConnection();
    QCoro::Task<void> saveToken();
    QCoro::Task<void> removeToken();
    void setStatus(const QString& message, bool error);
    void clearStatus();
    void updateButtons();

    core::HttpClient* m_http;
    core::TokenStore* m_tokens;
    config::RealDebridSettings& m_rdSettings;

    QLineEdit* m_tokenEdit {};
    QPushButton* m_showHideButton {};
    QPushButton* m_testButton {};
    QPushButton* m_saveButton {};
    QPushButton* m_removeButton {};
    QLabel* m_statusLabel {};
};

} // namespace kinema::ui::settings
