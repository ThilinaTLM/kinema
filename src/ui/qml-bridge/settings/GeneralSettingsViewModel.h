// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>
#include <QString>

namespace kinema::config {
class AppearanceSettings;
class SearchSettings;
}

namespace kinema::ui::qml::settings {

class GeneralSettingsViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(int defaultSearchKind READ defaultSearchKind
        WRITE setDefaultSearchKind NOTIFY defaultSearchKindChanged)
    Q_PROPERTY(bool closeToTray READ closeToTray
        WRITE setCloseToTray NOTIFY closeToTrayChanged)
    Q_PROPERTY(bool trayAvailable READ trayAvailable CONSTANT)

public:
    GeneralSettingsViewModel(config::SearchSettings& search,
        config::AppearanceSettings& appearance,
        QObject* parent = nullptr);

    int defaultSearchKind() const;
    bool closeToTray() const;
    bool trayAvailable() const;

    void setDefaultSearchKind(int kind);
    void setCloseToTray(bool on);

Q_SIGNALS:
    void defaultSearchKindChanged();
    void closeToTrayChanged();

private:
    config::SearchSettings& m_search;
    config::AppearanceSettings& m_appearance;
};

} // namespace kinema::ui::qml::settings
