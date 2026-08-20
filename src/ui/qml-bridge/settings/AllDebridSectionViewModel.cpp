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
#include "kinema_log_ui.h"
#include <KLocalizedString>

namespace kinema::ui::qml::settings {

// ============================== Debrid: AllDebrid section ===============

AllDebridSectionViewModel::AllDebridSectionViewModel(
    core::HttpClient* http, core::TokenStore* tokens,
    config::DebridSettings& settings, QObject* parent)
    : CredentialSectionViewModelBase(http, tokens, settings, parent)
{
}

QString AllDebridSectionViewModel::credentialKey() const
{
    return QString::fromLatin1(core::TokenStore::kAllDebridKey);
}

bool AllDebridSectionViewModel::isSaved() const
{
    return m_settings.allDebridConfigured();
}

void AllDebridSectionViewModel::setConfigured(bool configured)
{
    m_settings.setAllDebridConfigured(configured);
}

QString AllDebridSectionViewModel::errorContext() const
{
    return QStringLiteral("ad settings");
}

void AllDebridSectionViewModel::testConnection()
{
    auto t = testTask();
    Q_UNUSED(t);
}

QCoro::Task<void> AllDebridSectionViewModel::testTask()
{
    const auto apiKey = m_credential.trimmed();
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

} // namespace kinema::ui::qml::settings
