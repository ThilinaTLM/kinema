// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/TokenController.h"

#include "api/TmdbClient.h"
#include "config/RealDebridSettings.h"
#include "core/TmdbConfig.h"
#include "core/TokenStore.h"
#include "kinema_debug.h"

namespace kinema::controllers {

TokenController::TokenController(
    core::TokenStore* tokens,
    api::TmdbClient* tmdb,
    const config::RealDebridSettings& rdSettings,
    QObject* parent)
    : QObject(parent)
    , m_tokens(tokens)
    , m_tmdb(tmdb)
    , m_rdSettings(rdSettings)
{
}

void TokenController::loadAll()
{
    if (m_rdSettings.configured()) {
        auto t = loadRdTask();
        Q_UNUSED(t);
    }
    auto t = loadTmdbTask();
    Q_UNUSED(t);
}

void TokenController::refreshRealDebrid()
{
    auto t = loadRdTask();
    Q_UNUSED(t);
}

void TokenController::refreshTmdb()
{
    auto t = loadTmdbTask();
    Q_UNUSED(t);
}

QCoro::Task<void> TokenController::loadRdTask()
{
    QString next;
    try {
        next = co_await m_tokens->read(
            QString::fromLatin1(core::TokenStore::kRealDebridKey));
    } catch (const std::exception& e) {
        qCWarning(KINEMA) << "RD token read failed:" << e.what();
    }
    if (next != m_rdToken) {
        m_rdToken = std::move(next);
        Q_EMIT realDebridTokenChanged(m_rdToken);
    }
}

QCoro::Task<void> TokenController::loadTmdbTask()
{
    // User override wins if present; otherwise the compile-time
    // default; otherwise empty (Discover shows not-configured state).
    QString user;
    try {
        user = co_await m_tokens->read(
            QString::fromLatin1(core::TokenStore::kTmdbKey));
    } catch (const std::exception& e) {
        qCWarning(KINEMA) << "TMDB token read failed:" << e.what();
    }

    QString next;
    if (!user.isEmpty()) {
        next = std::move(user);
    } else {
        const auto* def = core::kTmdbCompiledDefaultToken;
        next = (def && def[0] != '\0') ? QString::fromLatin1(def)
                                       : QString {};
    }

    if (m_tmdb) {
        m_tmdb->setToken(next);
    }

    if (next != m_tmdbToken) {
        m_tmdbToken = std::move(next);
        Q_EMIT tmdbTokenChanged(m_tmdbToken);
    }
}

} // namespace kinema::controllers
