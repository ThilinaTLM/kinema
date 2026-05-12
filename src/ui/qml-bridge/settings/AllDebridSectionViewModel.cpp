// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/settings/AllDebridSectionViewModel.h"
#include "core/util/DateFormat.h"
#include "ui/qml-bridge/settings/SettingsStatus.h"
#include "api/AllDebridClient.h"
#include "config/DebridSettings.h"
#include "core/io/HttpClient.h"
#include "core/io/HttpError.h"
#include "core/io/HttpErrorPresenter.h"
#include "core/persistence/TokenStore.h"
#include "domain/Debrid.h"
#include "kinema_log_ui.h"
#include <KLocalizedString>

namespace kinema::ui::qml::settings {

// ============================== Debrid: AllDebrid section ===============

AllDebridSectionViewModel::AllDebridSectionViewModel(
    core::HttpClient* http, core::TokenStore* tokens,
    config::DebridSettings& settings, QObject* parent)
    : QObject(parent)
    , m_http(http)
    , m_tokens(tokens)
    , m_settings(settings)
{
}

bool AllDebridSectionViewModel::apiKeySaved() const
{
    return m_settings.allDebridConfigured();
}

void AllDebridSectionViewModel::setApiKey(const QString& apiKey)
{
    if (m_apiKey == apiKey) {
        return;
    }
    m_apiKey = apiKey;
    Q_EMIT apiKeyInputChanged();
}

void AllDebridSectionViewModel::load()
{
    auto t = loadTask();
    Q_UNUSED(t);
}
void AllDebridSectionViewModel::testConnection()
{
    auto t = testTask();
    Q_UNUSED(t);
}
void AllDebridSectionViewModel::save()
{
    auto t = saveTask();
    Q_UNUSED(t);
}
void AllDebridSectionViewModel::remove()
{
    auto t = removeTask();
    Q_UNUSED(t);
}

void AllDebridSectionViewModel::setStatus(const QString& message, int kind)
{
    if (m_statusMessage == message && m_statusKind == kind) {
        return;
    }
    m_statusMessage = message;
    m_statusKind = kind;
    Q_EMIT statusChanged();
}

void AllDebridSectionViewModel::setBusy(bool on)
{
    if (m_busy == on) {
        return;
    }
    m_busy = on;
    Q_EMIT busyChanged();
}

QCoro::Task<void> AllDebridSectionViewModel::loadTask()
{
    setBusy(true);
    try {
        const auto existing = co_await m_tokens->read(
            QString::fromLatin1(core::TokenStore::kAllDebridKey));
        if (!existing.isEmpty()) {
            setApiKey(existing);
        }
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "ad settings/load"),
            kStatusError);
    }
    setBusy(false);
}

QCoro::Task<void> AllDebridSectionViewModel::testTask()
{
    const auto apiKey = m_apiKey.trimmed();
    if (apiKey.isEmpty()) {
        co_return;
    }
    setBusy(true);
    setStatus(i18nc("@info ad settings status, in progress",
        "Testing AllDebrid API key…"), kStatusInfo);
    api::AllDebridClient client(m_http);
    client.setApiKey(apiKey);
    try {
        const auto user = co_await client.user();
        const auto plan = user.isPremium
            ? (user.isTrial
                ? i18nc("@info ad plan label", "trial")
                : i18nc("@info ad plan label", "premium"))
            : i18nc("@info ad plan label", "free");
        QString msg = i18nc("@info ad settings status",
            "Signed in as %1 (%2)",
            user.username.isEmpty() ? QStringLiteral("—")
                                    : user.username,
            plan);
        if (user.premiumUntil) {
            msg += QLatin1Char('\n')
                + i18nc("@info ad settings status premium expiry",
                    "Premium until: %1",
                    core::formatReleaseDate(*user.premiumUntil));
        }
        setStatus(msg, kStatusPositive);
    } catch (const std::exception& e) {
        if (const auto* he = core::asHttpError(e);
            he
            && (he->httpStatus() == 401 || he->httpStatus() == 403)) {
            setStatus(i18nc("@info ad settings status",
                "AllDebrid rejected the API key (HTTP %1).",
                he->httpStatus()),
                kStatusError);
        } else {
            setStatus(core::describeError(e, "ad settings/test"),
                kStatusError);
        }
    }
    setBusy(false);
}

QCoro::Task<void> AllDebridSectionViewModel::saveTask()
{
    const auto apiKey = m_apiKey.trimmed();
    if (apiKey.isEmpty()) {
        co_return;
    }
    setBusy(true);
    try {
        co_await m_tokens->write(
            QString::fromLatin1(core::TokenStore::kAllDebridKey),
            apiKey);
        m_settings.setAllDebridConfigured(true);
        Q_EMIT apiKeySavedChanged();
        Q_EMIT apiKeyChanged(apiKey);
        setStatus(i18nc("@info ad settings status",
            "API key saved to keyring."), kStatusPositive);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "ad settings/save"),
            kStatusError);
    }
    setBusy(false);
}

QCoro::Task<void> AllDebridSectionViewModel::removeTask()
{
    setBusy(true);
    try {
        co_await m_tokens->remove(
            QString::fromLatin1(core::TokenStore::kAllDebridKey));
        m_settings.setAllDebridConfigured(false);
        m_apiKey.clear();
        Q_EMIT apiKeyInputChanged();
        Q_EMIT apiKeySavedChanged();
        Q_EMIT apiKeyChanged(QString {});
        setStatus(i18nc("@info ad settings status",
            "API key removed from keyring."), kStatusInfo);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "ad settings/remove"),
            kStatusError);
    }
    setBusy(false);
}

} // namespace kinema::ui::qml::settings
