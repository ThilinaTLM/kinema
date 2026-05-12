// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/settings/RealDebridSectionViewModel.h"
#include "core/util/DateFormat.h"
#include "ui/qml-bridge/settings/SettingsStatus.h"
#include "api/RealDebridClient.h"
#include "config/DebridSettings.h"
#include "core/io/HttpClient.h"
#include "core/io/HttpError.h"
#include "core/io/HttpErrorPresenter.h"
#include "core/persistence/TokenStore.h"
#include "domain/Debrid.h"
#include "kinema_log_ui.h"
#include <KLocalizedString>

namespace kinema::ui::qml::settings {

// ============================== Debrid: Real-Debrid section ==============

RealDebridSectionViewModel::RealDebridSectionViewModel(
    core::HttpClient* http, core::TokenStore* tokens,
    config::DebridSettings& settings, QObject* parent)
    : QObject(parent)
    , m_http(http)
    , m_tokens(tokens)
    , m_settings(settings)
{
}

bool RealDebridSectionViewModel::tokenSaved() const
{
    return m_settings.realDebridConfigured();
}

void RealDebridSectionViewModel::setToken(const QString& token)
{
    if (m_token == token) {
        return;
    }
    m_token = token;
    Q_EMIT tokenInputChanged();
}

void RealDebridSectionViewModel::load()
{
    auto t = loadTask();
    Q_UNUSED(t);
}
void RealDebridSectionViewModel::testConnection()
{
    auto t = testTask();
    Q_UNUSED(t);
}
void RealDebridSectionViewModel::save()
{
    auto t = saveTask();
    Q_UNUSED(t);
}
void RealDebridSectionViewModel::remove()
{
    auto t = removeTask();
    Q_UNUSED(t);
}

void RealDebridSectionViewModel::setStatus(const QString& message, int kind)
{
    if (m_statusMessage == message && m_statusKind == kind) {
        return;
    }
    m_statusMessage = message;
    m_statusKind = kind;
    Q_EMIT statusChanged();
}

void RealDebridSectionViewModel::setBusy(bool on)
{
    if (m_busy == on) {
        return;
    }
    m_busy = on;
    Q_EMIT busyChanged();
}

QCoro::Task<void> RealDebridSectionViewModel::loadTask()
{
    setBusy(true);
    try {
        const auto existing = co_await m_tokens->read(
            QString::fromLatin1(core::TokenStore::kRealDebridKey));
        if (!existing.isEmpty()) {
            setToken(existing);
        }
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "rd settings/load"),
            kStatusError);
    }
    setBusy(false);
}

QCoro::Task<void> RealDebridSectionViewModel::testTask()
{
    const auto token = m_token.trimmed();
    if (token.isEmpty()) {
        co_return;
    }
    setBusy(true);
    setStatus(i18nc("@info rd settings status, in progress",
        "Testing Real-Debrid token…"), kStatusInfo);
    api::RealDebridClient client(m_http);
    client.setToken(token);
    try {
        const auto user = co_await client.user();
        QString msg = i18nc("@info rd settings status",
            "Signed in as %1 (%2)",
            user.username.isEmpty() ? QStringLiteral("—")
                                    : user.username,
            user.type.isEmpty() ? QStringLiteral("?") : user.type);
        if (user.premiumUntil) {
            msg += QLatin1Char('\n')
                + i18nc("@info rd settings status premium expiry",
                    "Premium until: %1",
                    core::formatReleaseDate(*user.premiumUntil));
        }
        setStatus(msg, kStatusPositive);
    } catch (const std::exception& e) {
        if (const auto* he = core::asHttpError(e);
            he
            && (he->httpStatus() == 401 || he->httpStatus() == 403)) {
            setStatus(i18nc("@info rd settings status",
                "Real-Debrid rejected the token (HTTP %1).",
                he->httpStatus()),
                kStatusError);
        } else {
            setStatus(core::describeError(e, "rd settings/test"),
                kStatusError);
        }
    }
    setBusy(false);
}

QCoro::Task<void> RealDebridSectionViewModel::saveTask()
{
    const auto token = m_token.trimmed();
    if (token.isEmpty()) {
        co_return;
    }
    setBusy(true);
    try {
        co_await m_tokens->write(
            QString::fromLatin1(core::TokenStore::kRealDebridKey),
            token);
        m_settings.setRealDebridConfigured(true);
        Q_EMIT tokenSavedChanged();
        Q_EMIT tokenChanged(token);
        setStatus(i18nc("@info rd settings status",
            "Token saved to keyring."), kStatusPositive);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "rd settings/save"),
            kStatusError);
    }
    setBusy(false);
}

QCoro::Task<void> RealDebridSectionViewModel::removeTask()
{
    setBusy(true);
    try {
        co_await m_tokens->remove(
            QString::fromLatin1(core::TokenStore::kRealDebridKey));
        m_settings.setRealDebridConfigured(false);
        m_token.clear();
        Q_EMIT tokenInputChanged();
        Q_EMIT tokenSavedChanged();
        Q_EMIT tokenChanged(QString {});
        setStatus(i18nc("@info rd settings status",
            "Token removed from keyring."), kStatusInfo);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "rd settings/remove"),
            kStatusError);
    }
    setBusy(false);
}

} // namespace kinema::ui::qml::settings
