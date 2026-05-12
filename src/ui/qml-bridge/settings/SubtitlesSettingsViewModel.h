// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>
#include <QString>
#include <QStringList>
#include <QVariantList>
#include <QCoro/QCoroTask>

namespace kinema::config {
class CacheSettings;
class SubtitleSettings;
}

namespace kinema::core {
class HttpClient;
class SubtitleCacheStore;
class TokenStore;
}

namespace kinema::ui::qml::settings {

class SubtitlesSettingsViewModel : public QObject
{
    Q_OBJECT

    Q_PROPERTY(QString apiKey READ apiKey WRITE setApiKey NOTIFY credentialInputChanged)
    Q_PROPERTY(QString username READ username WRITE setUsername NOTIFY credentialInputChanged)
    Q_PROPERTY(QString password READ password WRITE setPassword NOTIFY credentialInputChanged)
    Q_PROPERTY(bool credentialsSaved READ credentialsSaved NOTIFY credentialsSavedChanged)
    Q_PROPERTY(QString statusMessage READ statusMessage NOTIFY statusChanged)
    Q_PROPERTY(int statusKind READ statusKind NOTIFY statusChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)

    Q_PROPERTY(QStringList preferredLanguages READ preferredLanguages
        WRITE setPreferredLanguages NOTIFY preferredLanguagesChanged)
    Q_PROPERTY(QString hearingImpaired READ hearingImpaired
        WRITE setHearingImpaired NOTIFY hearingImpairedChanged)
    Q_PROPERTY(QString foreignPartsOnly READ foreignPartsOnly
        WRITE setForeignPartsOnly NOTIFY foreignPartsOnlyChanged)
    Q_PROPERTY(int subtitleBudgetMb READ subtitleBudgetMb
        WRITE setSubtitleBudgetMb NOTIFY subtitleBudgetMbChanged)

    Q_PROPERTY(QVariantList commonLanguages READ commonLanguages CONSTANT)

public:
    SubtitlesSettingsViewModel(core::HttpClient* http,
        core::TokenStore* tokens,
        config::SubtitleSettings& subtitleSettings,
        config::CacheSettings& cacheSettings,
        core::SubtitleCacheStore* subtitleCache,
        QObject* parent = nullptr);

    QString apiKey() const { return m_apiKey; }
    QString username() const { return m_username; }
    QString password() const { return m_password; }
    bool credentialsSaved() const { return m_credentialsSaved; }
    QString statusMessage() const { return m_statusMessage; }
    int statusKind() const { return m_statusKind; }
    bool busy() const { return m_busy; }

    QStringList preferredLanguages() const;
    QString hearingImpaired() const;
    QString foreignPartsOnly() const;
    int subtitleBudgetMb() const;
    QVariantList commonLanguages() const;

    void setApiKey(const QString& v);
    void setUsername(const QString& v);
    void setPassword(const QString& v);
    void setPreferredLanguages(const QStringList& codes);
    void setHearingImpaired(const QString& v);
    void setForeignPartsOnly(const QString& v);
    void setSubtitleBudgetMb(int v);

    Q_INVOKABLE QString languageDisplayName(const QString& code) const;
    Q_INVOKABLE void addLanguage(const QString& code);
    Q_INVOKABLE void removeLanguageAt(int index);
    Q_INVOKABLE void moveLanguage(int from, int to);

public Q_SLOTS:
    void load();
    void testConnection();
    void saveCredentials();
    void removeCredentials();
    void clearCache();

Q_SIGNALS:
    void credentialInputChanged();
    void credentialsSavedChanged();
    void statusChanged();
    void busyChanged();
    void preferredLanguagesChanged();
    void hearingImpairedChanged();
    void foreignPartsOnlyChanged();
    void subtitleBudgetMbChanged();
    void credentialsChanged();

private:
    void setStatus(const QString& message, int kind);
    void setBusy(bool on);
    QCoro::Task<void> loadTask();
    QCoro::Task<void> testTask();
    QCoro::Task<void> saveTask();
    QCoro::Task<void> removeTask();

    core::HttpClient* m_http;
    core::TokenStore* m_tokens;
    config::SubtitleSettings& m_subtitleSettings;
    config::CacheSettings& m_cacheSettings;
    core::SubtitleCacheStore* m_subtitleCache;
    QString m_apiKey;
    QString m_username;
    QString m_password;
    bool m_credentialsSaved = false;
    QString m_statusMessage;
    int m_statusKind = 0;
    bool m_busy = false;
};

} // namespace kinema::ui::qml::settings
