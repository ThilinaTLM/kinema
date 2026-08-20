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

/**
 * Shared credential lifecycle for the Real-Debrid and AllDebrid
 * settings sections. Both sections load/save/remove a single secret
 * in the system keyring, track a busy/status state, and expose the
 * same four QML slots. The only differences are the provider's
 * token-store key, its "configured" flag on `DebridSettings`, and
 * the connection test — all supplied by `CredentialSectionViewModel`.
 *
 * The generic `credential` / `credentialSaved` surface is re-exposed
 * by each provider section under its QML-facing names (`token` /
 * `apiKey`), so the shared machinery here emits the generic
 * `credential*Changed` signals.
 */
class CredentialSectionViewModelBase : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString statusMessage READ statusMessage NOTIFY statusChanged)
    Q_PROPERTY(int statusKind READ statusKind NOTIFY statusChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)

public:
    CredentialSectionViewModelBase(core::HttpClient* http,
        core::TokenStore* tokens,
        config::DebridSettings& settings,
        QObject* parent = nullptr);

    QString credential() const { return m_credential; }
    QString statusMessage() const { return m_statusMessage; }
    int statusKind() const { return m_statusKind; }
    bool busy() const { return m_busy; }

    void setCredential(const QString& credential);

public Q_SLOTS:
    void load();
    void save();
    void remove();

    /// Connection test, provider-specific (each section talks to its
    /// own client API). Base default is a no-op.
    virtual void testConnection() {}

Q_SIGNALS:
    void statusChanged();
    void busyChanged();
    void credentialInputChanged();
    void credentialSavedChanged();
    void credentialChanged(const QString& credential);

protected:
    // Provider hooks.
    virtual QString credentialKey() const = 0;
    virtual bool isSaved() const = 0;
    virtual void setConfigured(bool configured) = 0;
    /// Error-handling context, e.g. "rd settings". Callers build
    /// "<ctx>/load", "<ctx>/save" and "<ctx>/remove".
    virtual QString errorContext() const = 0;

    void setStatus(const QString& message, int kind);
    void setBusy(bool on);
    void emitSavedChanged();

    core::HttpClient* m_http;
    core::TokenStore* m_tokens;
    config::DebridSettings& m_settings;
    QString m_credential;
    QString m_statusMessage;
    int m_statusKind = 0;
    bool m_busy = false;

private:
    QCoro::Task<void> loadTask();
    QCoro::Task<void> saveTask();
    QCoro::Task<void> removeTask();
};

} // namespace kinema::ui::qml::settings
