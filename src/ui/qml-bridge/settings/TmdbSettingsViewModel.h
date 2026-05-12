// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>
#include <QString>
#include <QCoro/QCoroTask>

namespace kinema::core {
class HttpClient;
class TokenStore;
}

namespace kinema::ui::qml::settings {

class TmdbSettingsViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString token READ token WRITE setToken NOTIFY tokenInputChanged)
    Q_PROPERTY(bool userTokenSaved READ userTokenSaved NOTIFY userTokenSavedChanged)
    Q_PROPERTY(bool hasCompiledDefault READ hasCompiledDefault CONSTANT)
    Q_PROPERTY(QString statusMessage READ statusMessage NOTIFY statusChanged)
    /// 0 = info, 1 = positive, 2 = error. Maps to
    /// `Kirigami.MessageType` in QML.
    Q_PROPERTY(int statusKind READ statusKind NOTIFY statusChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)

public:
    TmdbSettingsViewModel(core::HttpClient* http,
        core::TokenStore* tokens, QObject* parent = nullptr);

    QString token() const { return m_token; }
    bool userTokenSaved() const { return m_userTokenSaved; }
    bool hasCompiledDefault() const;
    QString statusMessage() const { return m_statusMessage; }
    int statusKind() const { return m_statusKind; }
    bool busy() const { return m_busy; }

    void setToken(const QString& token);

public Q_SLOTS:
    void load();
    void testConnection();
    void save();
    void remove();

Q_SIGNALS:
    void tokenInputChanged();
    void userTokenSavedChanged();
    void statusChanged();
    void busyChanged();
    void tokenChanged(const QString& token);

private:
    void setStatus(const QString& message, int kind);
    void setBusy(bool on);
    QCoro::Task<void> loadTask();
    QCoro::Task<void> testTask();
    QCoro::Task<void> saveTask();
    QCoro::Task<void> removeTask();

    core::HttpClient* m_http;
    core::TokenStore* m_tokens;
    QString m_token;
    bool m_userTokenSaved = false;
    QString m_statusMessage;
    int m_statusKind = 0;
    bool m_busy = false;
};

} // namespace kinema::ui::qml::settings
