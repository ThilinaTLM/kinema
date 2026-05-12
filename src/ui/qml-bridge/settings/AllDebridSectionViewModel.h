// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>
#include <QString>
#include <QCoro/QCoroTask>

namespace kinema::config {
class DebridSettings;
}

namespace kinema::core {
class HttpClient;
class TokenStore;
}

namespace kinema::ui::qml::settings {

class AllDebridSectionViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString apiKey READ apiKey WRITE setApiKey NOTIFY apiKeyInputChanged)
    Q_PROPERTY(bool apiKeySaved READ apiKeySaved NOTIFY apiKeySavedChanged)
    Q_PROPERTY(QString statusMessage READ statusMessage NOTIFY statusChanged)
    Q_PROPERTY(int statusKind READ statusKind NOTIFY statusChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)

public:
    AllDebridSectionViewModel(core::HttpClient* http,
        core::TokenStore* tokens,
        config::DebridSettings& settings,
        QObject* parent = nullptr);

    QString apiKey() const { return m_apiKey; }
    bool apiKeySaved() const;
    QString statusMessage() const { return m_statusMessage; }
    int statusKind() const { return m_statusKind; }
    bool busy() const { return m_busy; }

    void setApiKey(const QString& apiKey);

public Q_SLOTS:
    void load();
    void testConnection();
    void save();
    void remove();

Q_SIGNALS:
    void apiKeyInputChanged();
    void apiKeySavedChanged();
    void statusChanged();
    void busyChanged();
    void apiKeyChanged(const QString& apiKey);

private:
    void setStatus(const QString& message, int kind);
    void setBusy(bool on);
    QCoro::Task<void> loadTask();
    QCoro::Task<void> testTask();
    QCoro::Task<void> saveTask();
    QCoro::Task<void> removeTask();

    core::HttpClient* m_http;
    core::TokenStore* m_tokens;
    config::DebridSettings& m_settings;
    QString m_apiKey;
    QString m_statusMessage;
    int m_statusKind = 0;
    bool m_busy = false;
};

} // namespace kinema::ui::qml::settings
