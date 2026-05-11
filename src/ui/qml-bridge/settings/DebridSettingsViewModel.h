// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>
#include "ui/qml-bridge/settings/AllDebridSectionViewModel.h"
#include "ui/qml-bridge/settings/RealDebridSectionViewModel.h"

namespace kinema::config {
class DebridSettings;
}

namespace kinema::core {
class HttpClient;
class TokenStore;
}

namespace kinema::ui::qml::settings {

class DebridSettingsViewModel : public QObject
{
    Q_OBJECT
    /// 0 = None, 1 = Real-Debrid, 2 = AllDebrid. Maps to
    /// `domain::DebridProvider`.
    Q_PROPERTY(int activeProvider READ activeProvider
        WRITE setActiveProvider NOTIFY activeProviderChanged)
    Q_PROPERTY(RealDebridSectionViewModel* realDebrid
        READ realDebrid CONSTANT)
    Q_PROPERTY(AllDebridSectionViewModel* allDebrid
        READ allDebrid CONSTANT)

public:
    DebridSettingsViewModel(core::HttpClient* http,
        core::TokenStore* tokens,
        config::DebridSettings& settings,
        QObject* parent = nullptr);

    int activeProvider() const;
    void setActiveProvider(int provider);

    RealDebridSectionViewModel* realDebrid() const { return m_rd; }
    AllDebridSectionViewModel* allDebrid() const { return m_ad; }

Q_SIGNALS:
    void activeProviderChanged();

private:
    config::DebridSettings& m_settings;
    RealDebridSectionViewModel* m_rd {};
    AllDebridSectionViewModel* m_ad {};
};

} // namespace kinema::ui::qml::settings
