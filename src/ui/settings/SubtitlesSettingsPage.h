// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QWidget>

#include <QCoro/QCoroTask>

class QCheckBox;
class QComboBox;
class QLabel;
class QLineEdit;
class QListWidget;
class QPushButton;
class QSpinBox;

namespace kinema::api {
class OpenSubtitlesClient;
}

namespace kinema::config {
class CacheSettings;
class SubtitleSettings;
}

namespace kinema::core {
class HttpClient;
class TokenStore;
class SubtitleCacheStore;
}

namespace kinema::ui::settings {

/**
 * Settings page for OpenSubtitles.com integration.
 *
 * Self-contained: API key + username + password are saved through
 * `core::TokenStore` (system keyring) when the user clicks "Save
 * credentials". A "Test connection" button validates the triple by
 * doing a real `POST /login` followed by a probe `GET /subtitles`.
 *
 * The page also exposes:
 *   - preferred languages (ordered ISO 639-2 list),
 *   - HI / Foreign-parts-only default modes,
 *   - subtitle cache budget (MB),
 *   - a "Clear subtitle cache" button.
 *
 * `tokenChanged()` notifies the dialog that
 * `TokenController::refreshOpenSubtitlesCredentials()` should be
 * triggered.
 */
class SubtitlesSettingsPage : public QWidget
{
    Q_OBJECT
public:
    SubtitlesSettingsPage(core::HttpClient* http,
        core::TokenStore* tokens,
        config::SubtitleSettings& subtitleSettings,
        config::CacheSettings& cacheSettings,
        core::SubtitleCacheStore* subtitleCache,
        QWidget* parent = nullptr);

    /// Apply the non-credential preferences (languages, filters,
    /// cache budget) to KSharedConfig. Called by SettingsDialog::Apply.
    void apply();
    void resetToDefaults();

Q_SIGNALS:
    /// Fired after a successful Save / Remove of credentials.
    void tokenChanged();

private:
    QCoro::Task<void> loadExistingTokens();
    QCoro::Task<void> testConnection();
    QCoro::Task<void> saveCredentials();
    QCoro::Task<void> removeCredentials();
    void onClearCache();
    void setStatus(const QString& message, bool error);
    void clearStatus();
    void updateButtons();
    void addLanguageRow();
    void removeSelectedLanguage();
    void moveLanguageUp();
    void moveLanguageDown();

    core::HttpClient* m_http;
    core::TokenStore* m_tokens;
    config::SubtitleSettings& m_subtitleSettings;
    config::CacheSettings& m_cacheSettings;
    core::SubtitleCacheStore* m_subtitleCache;

    QLineEdit* m_apiKeyEdit {};
    QLineEdit* m_usernameEdit {};
    QLineEdit* m_passwordEdit {};
    QPushButton* m_testButton {};
    QPushButton* m_saveButton {};
    QPushButton* m_removeButton {};
    QLabel* m_statusLabel {};

    QListWidget* m_langList {};
    QComboBox* m_addLangCombo {};
    QPushButton* m_addLangButton {};
    QPushButton* m_removeLangButton {};
    QPushButton* m_upLangButton {};
    QPushButton* m_downLangButton {};

    QComboBox* m_hiCombo {};
    QComboBox* m_fpoCombo {};
    QSpinBox* m_cacheBudgetSpin {};
    QPushButton* m_clearCacheButton {};

    bool m_credentialsSaved = false;
};

} // namespace kinema::ui::settings
