// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include <QDialog>

#include <QCoro/QCoroTask>

class QDialogButtonBox;
class QLabel;
class QLineEdit;
class QPushButton;

namespace kinema::core {
class HttpClient;
class TokenStore;
}

namespace kinema::ui {

/**
 * Dialog used to paste, test, save, and remove the Real-Debrid API
 * token. The token is never persisted in KConfig — only in the system
 * keyring via TokenStore.
 *
 * Emits `tokenChanged(newToken)` on Save so MainWindow can update its
 * in-memory RD token immediately (without waiting for another keyring
 * round-trip). An empty new token means the user clicked "Remove".
 */
class RealDebridDialog : public QDialog
{
    Q_OBJECT
public:
    RealDebridDialog(core::HttpClient* http, core::TokenStore* tokens,
        QWidget* parent = nullptr);

Q_SIGNALS:
    /// Fired after a successful Save or Remove. An empty string means
    /// the user removed their token.
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

    QLineEdit* m_tokenEdit {};
    QPushButton* m_showHideButton {};
    QPushButton* m_testButton {};
    QPushButton* m_removeButton {};
    QLabel* m_statusLabel {};
    QDialogButtonBox* m_buttonBox {};
};

} // namespace kinema::ui
