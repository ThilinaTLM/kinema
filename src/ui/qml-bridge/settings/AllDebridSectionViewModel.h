// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "ui/qml-bridge/settings/CredentialSectionViewModelBase.h"

#include <QCoro/QCoroTask>

namespace kinema::ui::qml::settings {

/**
 * AllDebrid credential section. Inherits the keyring load/save/
 * remove lifecycle from `CredentialSectionViewModelBase`; only the
 * connection test is provider-specific.
 */
class AllDebridSectionViewModel : public CredentialSectionViewModelBase
{
    Q_OBJECT
    Q_PROPERTY(QString apiKey READ credential WRITE setCredential
        NOTIFY credentialInputChanged)
    Q_PROPERTY(bool apiKeySaved READ isSaved NOTIFY credentialSavedChanged)

public:
    AllDebridSectionViewModel(core::HttpClient* http,
        core::TokenStore* tokens,
        config::DebridSettings& settings,
        QObject* parent = nullptr);

public Q_SLOTS:
    void testConnection() override;

protected:
    QString credentialKey() const override;
    bool isSaved() const override;
    void setConfigured(bool configured) override;
    QString errorContext() const override;

private:
    QCoro::Task<void> testTask();
};

} // namespace kinema::ui::qml::settings
