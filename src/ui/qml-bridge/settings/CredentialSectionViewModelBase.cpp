// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/settings/CredentialSectionViewModelBase.h"

#include "ui/qml-bridge/settings/SettingsStatus.h"
#include "config/DebridSettings.h"
#include "core/io/HttpError.h"
#include "core/io/HttpErrorPresenter.h"
#include "core/persistence/TokenStore.h"
#include "kinema_log_ui.h"
#include <KLocalizedString>

#include <QByteArray>
#include <QPointer>

namespace kinema::ui::qml::settings {

CredentialSectionViewModelBase::CredentialSectionViewModelBase(
    core::HttpClient* http, core::TokenStore* tokens,
    config::DebridSettings& settings, QObject* parent)
    : QObject(parent)
    , m_http(http)
    , m_tokens(tokens)
    , m_settings(settings)
{
}

void CredentialSectionViewModelBase::setCredential(const QString& credential)
{
    if (m_credential == credential) {
        return;
    }
    m_credential = credential;
    Q_EMIT credentialInputChanged();
}

void CredentialSectionViewModelBase::setStatus(const QString& message, int kind)
{
    if (m_statusMessage == message && m_statusKind == kind) {
        return;
    }
    m_statusMessage = message;
    m_statusKind = kind;
    Q_EMIT statusChanged();
}

void CredentialSectionViewModelBase::setBusy(bool on)
{
    if (m_busy == on) {
        return;
    }
    m_busy = on;
    Q_EMIT busyChanged();
}

void CredentialSectionViewModelBase::emitSavedChanged()
{
    Q_EMIT credentialSavedChanged();
}

void CredentialSectionViewModelBase::load()
{
    auto t = loadTask();
    Q_UNUSED(t);
}

void CredentialSectionViewModelBase::save()
{
    auto t = saveTask();
    Q_UNUSED(t);
}

void CredentialSectionViewModelBase::remove()
{
    auto t = removeTask();
    Q_UNUSED(t);
}

QCoro::Task<void> CredentialSectionViewModelBase::loadTask()
{
    QPointer<CredentialSectionViewModelBase> self(this);
    setBusy(true);
    try {
        const auto existing = co_await m_tokens->read(credentialKey());
        if (!self) {
            co_return;
        }
        if (!existing.isEmpty()) {
            setCredential(existing);
        }
    } catch (const core::TokenStoreError& e) {
        if (!self) {
            co_return;
        }
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        if (!self) {
            co_return;
        }
        const QByteArray ctx = (errorContext() + QStringLiteral("/load")).toUtf8();
        setStatus(core::describeError(e, ctx.constData()), kStatusError);
    }
    if (self) {
        setBusy(false);
    }
}

QCoro::Task<void> CredentialSectionViewModelBase::saveTask()
{
    const auto credential = m_credential.trimmed();
    if (credential.isEmpty()) {
        co_return;
    }
    setBusy(true);
    try {
        co_await m_tokens->write(credentialKey(), credential);
        setConfigured(true);
        emitSavedChanged();
        Q_EMIT credentialChanged(credential);
        setStatus(i18nc("@info settings status",
            "Credential saved to keyring."), kStatusPositive);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        const QByteArray ctx = (errorContext() + QStringLiteral("/save")).toUtf8();
        setStatus(core::describeError(e, ctx.constData()), kStatusError);
    }
    setBusy(false);
}

QCoro::Task<void> CredentialSectionViewModelBase::removeTask()
{
    setBusy(true);
    try {
        co_await m_tokens->remove(credentialKey());
        setConfigured(false);
        m_credential.clear();
        Q_EMIT credentialInputChanged();
        emitSavedChanged();
        Q_EMIT credentialChanged(QString {});
        setStatus(i18nc("@info settings status",
            "Credential removed from keyring."), kStatusInfo);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        const QByteArray ctx = (errorContext() + QStringLiteral("/remove")).toUtf8();
        setStatus(core::describeError(e, ctx.constData()), kStatusError);
    }
    setBusy(false);
}

} // namespace kinema::ui::qml::settings
