// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/settings/TmdbSettingsViewModel.h"
#include "ui/qml-bridge/settings/SettingsStatus.h"
#include "api/TmdbClient.h"
#include "core/io/HttpClient.h"
#include "core/io/HttpError.h"
#include "core/io/HttpErrorPresenter.h"
#include "core/persistence/TokenStore.h"
#include "core/TmdbConfig.h"
#include "kinema_log_ui.h"
#include <KLocalizedString>

namespace kinema::ui::qml::settings {

// ============================== TMDB ======================================

TmdbSettingsViewModel::TmdbSettingsViewModel(
    core::HttpClient* http, core::TokenStore* tokens, QObject* parent)
    : QObject(parent)
    , m_http(http)
    , m_tokens(tokens)
{
}

bool TmdbSettingsViewModel::hasCompiledDefault() const
{
    return core::kTmdbCompiledDefaultToken != nullptr
        && core::kTmdbCompiledDefaultToken[0] != '\0';
}

void TmdbSettingsViewModel::setToken(const QString& token)
{
    if (m_token == token) {
        return;
    }
    m_token = token;
    Q_EMIT tokenInputChanged();
}

void TmdbSettingsViewModel::load()
{
    auto t = loadTask();
    Q_UNUSED(t);
}

void TmdbSettingsViewModel::testConnection()
{
    auto t = testTask();
    Q_UNUSED(t);
}

void TmdbSettingsViewModel::save()
{
    auto t = saveTask();
    Q_UNUSED(t);
}

void TmdbSettingsViewModel::remove()
{
    auto t = removeTask();
    Q_UNUSED(t);
}

void TmdbSettingsViewModel::setStatus(const QString& message, int kind)
{
    if (m_statusMessage == message && m_statusKind == kind) {
        return;
    }
    m_statusMessage = message;
    m_statusKind = kind;
    Q_EMIT statusChanged();
}

void TmdbSettingsViewModel::setBusy(bool on)
{
    if (m_busy == on) {
        return;
    }
    m_busy = on;
    Q_EMIT busyChanged();
}

QCoro::Task<void> TmdbSettingsViewModel::loadTask()
{
    setBusy(true);
    try {
        const auto existing = co_await m_tokens->read(
            QString::fromLatin1(core::TokenStore::kTmdbKey));
        if (!existing.isEmpty()) {
            setToken(existing);
            if (!m_userTokenSaved) {
                m_userTokenSaved = true;
                Q_EMIT userTokenSavedChanged();
            }
            setStatus(i18nc("@info tmdb settings status",
                "Using your token."), kStatusPositive);
        } else if (hasCompiledDefault()) {
            setStatus(i18nc("@info tmdb settings status",
                "Using the bundled token."), kStatusInfo);
        } else {
            setStatus(i18nc("@info tmdb settings status",
                "No token configured. Discover is disabled."),
                kStatusError);
        }
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "tmdb settings/load"),
            kStatusError);
    }
    setBusy(false);
}

QCoro::Task<void> TmdbSettingsViewModel::testTask()
{
    QString token = m_token.trimmed();
    if (token.isEmpty()) {
        if (m_userTokenSaved) {
            try {
                token = co_await m_tokens->read(
                    QString::fromLatin1(core::TokenStore::kTmdbKey));
            } catch (const std::exception& e) {
                setStatus(
                    core::describeError(e,
                        "tmdb settings/read saved token"),
                    kStatusError);
                co_return;
            }
        }
        if (token.isEmpty() && hasCompiledDefault()) {
            token
                = QString::fromLatin1(core::kTmdbCompiledDefaultToken);
        }
    }
    if (token.isEmpty()) {
        setStatus(i18nc("@info tmdb settings status",
            "No token to test. Paste one above or build with "
            "KINEMA_TMDB_DEFAULT_TOKEN."),
            kStatusError);
        co_return;
    }

    setBusy(true);
    setStatus(i18nc("@info tmdb settings status, in progress",
        "Testing TMDB credentials…"), kStatusInfo);
    api::TmdbClient client(m_http);
    client.setToken(token);
    try {
        co_await client.testAuth();
        setStatus(i18nc("@info tmdb settings status",
            "Connected to TMDB."), kStatusPositive);
    } catch (const std::exception& e) {
        if (const auto* he = core::asHttpError(e);
            he
            && (he->httpStatus() == 401 || he->httpStatus() == 403)) {
            setStatus(i18nc("@info tmdb settings status",
                "TMDB rejected the token (HTTP %1).",
                he->httpStatus()),
                kStatusError);
        } else {
            setStatus(core::describeError(e,
                          "tmdb settings/test auth"),
                kStatusError);
        }
    }
    setBusy(false);
}

QCoro::Task<void> TmdbSettingsViewModel::saveTask()
{
    const auto token = m_token.trimmed();
    if (token.isEmpty()) {
        co_return;
    }
    setBusy(true);
    try {
        co_await m_tokens->write(
            QString::fromLatin1(core::TokenStore::kTmdbKey), token);
        if (!m_userTokenSaved) {
            m_userTokenSaved = true;
            Q_EMIT userTokenSavedChanged();
        }
        Q_EMIT tokenChanged(token);
        setStatus(i18nc("@info tmdb settings status",
            "Token saved to keyring. Using your token."),
            kStatusPositive);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "tmdb settings/save"),
            kStatusError);
    }
    setBusy(false);
}

QCoro::Task<void> TmdbSettingsViewModel::removeTask()
{
    setBusy(true);
    try {
        co_await m_tokens->remove(
            QString::fromLatin1(core::TokenStore::kTmdbKey));
        m_token.clear();
        Q_EMIT tokenInputChanged();
        if (m_userTokenSaved) {
            m_userTokenSaved = false;
            Q_EMIT userTokenSavedChanged();
        }
        Q_EMIT tokenChanged(QString {});
        setStatus(hasCompiledDefault()
                ? i18nc("@info tmdb settings status",
                      "Token removed. Falling back to bundled "
                      "token.")
                : i18nc("@info tmdb settings status",
                      "Token removed. No token configured."),
            kStatusInfo);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "tmdb settings/remove"),
            kStatusError);
    }
    setBusy(false);
}

} // namespace kinema::ui::qml::settings
