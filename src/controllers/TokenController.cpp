// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/TokenController.h"

#include "api/TmdbClient.h"
#include "config/RealDebridSettings.h"
#include "core/TmdbConfig.h"
#include "core/TokenStore.h"
#include "kinema_debug.h"

namespace kinema::controllers {

namespace {

QCoro::Task<QString> safeRead(core::TokenStore& store, const char* key,
    const char* label)
{
    try {
        co_return co_await store.read(QString::fromLatin1(key));
    } catch (const std::exception& e) {
        qCWarning(KINEMA) << label << "read failed:" << e.what();
    }
    co_return QString {};
}

} // namespace

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
        [[maybe_unused]] auto rd = loadRdTask();
    }
    [[maybe_unused]] auto tmdb = loadTmdbTask();
    [[maybe_unused]] auto os = loadOpenSubtitlesTask();
}

void TokenController::refreshRealDebrid()
{
    [[maybe_unused]] auto t = loadRdTask();
}

void TokenController::refreshTmdb()
{
    [[maybe_unused]] auto t = loadTmdbTask();
}

void TokenController::refreshOpenSubtitlesCredentials()
{
    [[maybe_unused]] auto t = loadOpenSubtitlesTask();
}

QCoro::Task<void> TokenController::loadRdTask()
{
    auto next = co_await safeRead(*m_tokens,
        core::TokenStore::kRealDebridKey, "RD token");
    if (next != m_rdToken) {
        m_rdToken = std::move(next);
        Q_EMIT realDebridTokenChanged(m_rdToken);
    }
}

QCoro::Task<void> TokenController::loadTmdbTask()
{
    // User override wins if present; otherwise the compile-time
    // default; otherwise empty (Discover shows not-configured state).
    auto user = co_await safeRead(*m_tokens,
        core::TokenStore::kTmdbKey, "TMDB token");

    QString next = user.isEmpty() ? m_tmdbCompiledDefaultToken
                                  : std::move(user);

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
    auto apiKey = co_await safeRead(*m_tokens,
        core::TokenStore::kOpenSubtitlesApiKey, "OpenSubtitles api-key");
    auto username = co_await safeRead(*m_tokens,
        core::TokenStore::kOpenSubtitlesUsername, "OpenSubtitles username");
    auto password = co_await safeRead(*m_tokens,
        core::TokenStore::kOpenSubtitlesPassword, "OpenSubtitles password");

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
