// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include <QObject>
#include <QString>

#include <QCoro/QCoroTask>

#include <stdexcept>

namespace kinema::core {

/**
 * Exception thrown by TokenStore write/remove when the keyring refuses
 * the operation. Callers catch this at the UI coroutine boundary and
 * surface the message inline (no modal error).
 */
class TokenStoreError : public std::runtime_error
{
public:
    explicit TokenStoreError(QString message)
        : std::runtime_error(message.toStdString())
        , m_message(std::move(message))
    {
    }
    const QString& message() const noexcept { return m_message; }

private:
    QString m_message;
};

/**
 * Thin QtKeychain wrapper returning QCoro::Task<T>.
 *
 * All secrets live under a single service name (`dev.tlmtech.kinema`) so
 * they show up as one group in kwalletmanager / gnome-keyring's
 * seahorse. Nothing is ever written to disk as plaintext; if the
 * platform has no keychain backend, jobs fail and `available()` flips
 * to false after the first failed probe.
 *
 * Reads return an empty string on miss (a user who has never set a
 * token is not an error). Writes and deletes throw TokenStoreError on
 * failure so the dialog can surface a meaningful message.
 */
class TokenStore : public QObject
{
    Q_OBJECT
public:
    static constexpr auto kServiceName = "dev.tlmtech.kinema";
    static constexpr auto kRealDebridKey = "realdebrid-token";
    static constexpr auto kTmdbKey = "tmdb-token";

    explicit TokenStore(QObject* parent = nullptr);

    /// Returns the stored value, or an empty string on miss.
    /// Throws TokenStoreError on keyring backend errors (not on miss).
    QCoro::Task<QString> read(QString key);

    /// Throws TokenStoreError on failure.
    QCoro::Task<void> write(QString key, QString value);

    /// Silently succeeds if the entry doesn't exist.
    /// Throws TokenStoreError on other failures.
    QCoro::Task<void> remove(QString key);

    /// Returns false if the last keyring operation reported NoBackendAvailable.
    /// Checked by the dialog to disable Save with a clear inline message.
    bool available() const noexcept { return m_available; }

private:
    mutable bool m_available = true;
};

} // namespace kinema::core
