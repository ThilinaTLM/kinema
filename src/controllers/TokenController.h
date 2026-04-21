// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>
#include <QString>

#include <QCoro/QCoroTask>

namespace kinema::api {
class TmdbClient;
}

namespace kinema::core {
class TokenStore;
}

namespace kinema::config {
class RealDebridSettings;
}

namespace kinema::controllers {

/**
 * Owns the in-memory copies of the Real-Debrid and TMDB tokens. One
 * TokenController per app; controllers that need the tokens receive
 * them as `const QString&` aliases and get live updates without any
 * keyring round-trip.
 *
 * TMDB resolution order:
 *   1. User-override token in the keyring (if any).
 *   2. Compile-time default (from core/TmdbConfig.h).
 *   3. Empty \u2014 Discover shows a \"not configured\" state.
 *
 * RD: presence of a keyring entry + the `configured` flag in
 * RealDebridSettings.
 */
class TokenController : public QObject
{
    Q_OBJECT
public:
    TokenController(
        core::TokenStore* tokens,
        api::TmdbClient* tmdb,
        const config::RealDebridSettings& rdSettings,
        QObject* parent = nullptr);

    const QString& realDebridToken() const noexcept { return m_rdToken; }
    const QString& tmdbToken() const noexcept { return m_tmdbToken; }

public Q_SLOTS:
    /// Kick off both keyring reads. Fire-and-forget.
    void loadAll();

    /// Re-read RD from the keyring (called after Save / Remove from
    /// the Real-Debrid settings page).
    void refreshRealDebrid();

    /// Re-resolve the TMDB token chain (user keyring \u2192 compile-time
    /// default). Called after Save / Remove from the TMDB settings page.
    void refreshTmdb();

Q_SIGNALS:
    /// Fires whenever the effective RD token changes. Empty string =
    /// token removed or never set.
    void realDebridTokenChanged(const QString&);

    /// Fires whenever the effective TMDB token changes. Empty string
    /// = no user override and no compile-time default.
    void tmdbTokenChanged(const QString&);

private:
    QCoro::Task<void> loadRdTask();
    QCoro::Task<void> loadTmdbTask();

    core::TokenStore* m_tokens;
    api::TmdbClient* m_tmdb;
    const config::RealDebridSettings& m_rdSettings;

    QString m_rdToken;
    QString m_tmdbToken;
};

} // namespace kinema::controllers
