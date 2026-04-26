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
    QObject* parent,
    QString tmdbCompiledDefaultToken)
    : QObject(parent)
    , m_tokens(tokens)
    , m_tmdb(tmdb)
    , m_rdSettings(rdSettings)
    , m_tmdbCompiledDefaultToken(tmdbCompiledDefaultToken.isNull()
            ? QString::fromLatin1(core::kTmdbCompiledDefaultToken)
            : std::move(tmdbCompiledDefaultToken))
{
}

void TokenController::loadAll()
{
    if (m_rdSettings.configured()) {
        auto t = loadRdTask();
        Q_UNUSED(t);
    }
    {
        auto t = loadTmdbTask();
        Q_UNUSED(t);
    }
    auto t = loadOpenSubtitlesTask();
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

void TokenController::refreshOpenSubtitlesCredentials()
{
    auto t = loadOpenSubtitlesTask();
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
        next = m_tmdbCompiledDefaultToken;
    }

    if (m_tmdb) {
        m_tmdb->setToken(next);
    }

    if (next != m_tmdbToken) {
        m_tmdbToken = std::move(next);
        Q_EMIT tmdbTokenChanged(m_tmdbToken);
    }
}

QCoro::Task<void> TokenController::loadOpenSubtitlesTask()
{
    QString apiKey, username, password;
    try {
        apiKey = co_await m_tokens->read(
            QString::fromLatin1(core::TokenStore::kOpenSubtitlesApiKey));
    } catch (const std::exception& e) {
        qCWarning(KINEMA) << "OpenSubtitles api-key read failed:" << e.what();
    }
    try {
        username = co_await m_tokens->read(
            QString::fromLatin1(core::TokenStore::kOpenSubtitlesUsername));
    } catch (const std::exception& e) {
        qCWarning(KINEMA) << "OpenSubtitles username read failed:" << e.what();
    }
    try {
        password = co_await m_tokens->read(
            QString::fromLatin1(core::TokenStore::kOpenSubtitlesPassword));
    } catch (const std::exception& e) {
        qCWarning(KINEMA) << "OpenSubtitles password read failed:" << e.what();
    }

    if (apiKey != m_osApiKey) {
        m_osApiKey = std::move(apiKey);
        Q_EMIT openSubtitlesApiKeyChanged(m_osApiKey);
    }
    if (username != m_osUsername) {
        m_osUsername = std::move(username);
        Q_EMIT openSubtitlesUsernameChanged(m_osUsername);
    }
    if (password != m_osPassword) {
        m_osPassword = std::move(password);
        Q_EMIT openSubtitlesPasswordChanged(m_osPassword);
    }
}

} // namespace kinema::controllers
