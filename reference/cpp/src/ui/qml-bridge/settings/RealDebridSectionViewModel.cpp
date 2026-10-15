// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/settings/RealDebridSectionViewModel.h"

#include "api/realdebrid/RealDebridClient.h"
#include "config/DebridSettings.h"
#include "core/io/HttpClient.h"
#include "core/io/HttpError.h"
#include "core/io/HttpErrorPresenter.h"
#include "core/persistence/TokenStore.h"
#include "core/util/DateFormat.h"
#include "kinema_log_ui.h"
#include "ui/qml-bridge/settings/SettingsStatus.h"

#include <KLocalizedString>

namespace kinema::ui::qml::settings {

// ============================== Debrid: Real-Debrid section ==============

RealDebridSectionViewModel::RealDebridSectionViewModel(core::HttpClient* http,
                                                       core::TokenStore* tokens,
                                                       config::DebridSettings& settings,
                                                       QObject* parent)
    : CredentialSectionViewModelBase(http, tokens, settings, parent)
{ }

QString RealDebridSectionViewModel::credentialKey() const
{
    return QString::fromLatin1(core::TokenStore::kRealDebridKey);
}

bool RealDebridSectionViewModel::isSaved() const
{
    return m_settings.realDebridConfigured();
}

void RealDebridSectionViewModel::setConfigured(bool configured)
{
    m_settings.setRealDebridConfigured(configured);
}

QString RealDebridSectionViewModel::errorContext() const
{
    return QStringLiteral("rd settings");
}

void RealDebridSectionViewModel::testConnection()
{
    auto t = testTask();
    Q_UNUSED(t);
}

QCoro::Task<void> RealDebridSectionViewModel::testTask()
{
    const auto token = m_credential.trimmed();
    if (token.isEmpty()) {
        co_return;
    }
    setBusy(true);
    setStatus(i18nc("@info rd settings status, in progress", "Testing Real-Debrid token…"),
              kStatusInfo);
    api::RealDebridClient client(m_http);
    client.setToken(token);
    try {
        const auto user = co_await client.user();
        QString msg = i18nc("@info rd settings status",
                            "Signed in as %1 (%2)",
                            user.username.isEmpty() ? QStringLiteral("—") : user.username,
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
            he && (he->httpStatus() == 401 || he->httpStatus() == 403)) {
            setStatus(i18nc("@info rd settings status",
                            "Real-Debrid rejected the token (HTTP %1).",
                            he->httpStatus()),
                      kStatusError);
        } else {
            setStatus(core::describeError(e, "rd settings/test"), kStatusError);
        }
    }
    setBusy(false);
}

} // namespace kinema::ui::qml::settings
