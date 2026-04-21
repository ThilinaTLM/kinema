// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

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

namespace kinema::ui::settings {

/**
 * Settings page for the TMDB integration that powers Discover.
 *
 * Unlike Real-Debrid, TMDB is *not* a per-user-required integration:
 * Discover works out of the box against a bundled token shipped at
 * build time (see KINEMA_TMDB_DEFAULT_TOKEN). This page lets a user
 * opt into their own token if they prefer — for privacy, to avoid
 * sharing a rate-limit pool, or as an escape hatch if the bundled
 * token has been revoked.
 *
 * UI flow mirrors RealDebridSettingsPage (Test / Save / Remove as
 * self-contained inline buttons; not part of the dialog's Apply/OK
 * flow), but the intro copy and status messages frame it as an
 * override rather than a setup requirement.
 *
 * Emits tokenChanged(token) on Save (non-empty) or Remove (empty).
 * MainWindow listens and re-resolves the effective token (user →
 * compile-time default → empty) in response.
 */
class TmdbSettingsPage : public QWidget
{
    Q_OBJECT
public:
    TmdbSettingsPage(core::HttpClient* http,
        core::TokenStore* tokens, QWidget* parent = nullptr);

Q_SIGNALS:
    /// Fired after a successful Save (non-empty user token) or Remove
    /// (empty string — in which case MainWindow falls back to the
    /// compile-time default).
    void tokenChanged(const QString& token);

private:
    QCoro::Task<void> loadExistingToken();
    QCoro::Task<void> testConnection();
    QCoro::Task<void> saveToken();
    QCoro::Task<void> removeToken();
    void setStatus(const QString& message, bool error);
    void clearStatus();
    void updateButtons();

    // Describe which token is currently active based on the empty-
    // or-not state of the user-supplied token and the compile-time
    // default. Used to populate the status line on initial load and
    // after Save / Remove.
    QString effectiveTokenSummary(bool userTokenPresent) const;

    core::HttpClient* m_http;
    core::TokenStore* m_tokens;

    QLineEdit* m_tokenEdit {};
    QPushButton* m_showHideButton {};
    QPushButton* m_testButton {};
    QPushButton* m_saveButton {};
    QPushButton* m_removeButton {};
    QLabel* m_statusLabel {};

    /// True if the keyring currently holds a user-supplied token.
    /// Tracked so Remove is only enabled when there's something to
    /// remove (the edit field alone isn't enough — the user might
    /// have typed a new one without saving).
    bool m_userTokenSaved = false;
};

} // namespace kinema::ui::settings
