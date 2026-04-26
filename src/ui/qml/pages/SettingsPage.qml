// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.settings as KirigamiSettings

import dev.tlmtech.kinema.app

// Settings host. `Kirigami.CategorizedSettings` is itself a
// `Kirigami.PageRow` — it renders the category list on the left
// and the active sub-page on the right (or stacks them on narrow
// screens). Sub-pages are `FormCard.FormCardPage`s.
KirigamiSettings.CategorizedSettings {
    id: page
    objectName: "settings"

    defaultPage: settingsVm.initialCategory.length > 0
        ? settingsVm.initialCategory
        : "general"

    actions: [
        KirigamiSettings.SettingAction {
            actionName: "general"
            text: i18nc("@title:tab settings page", "General")
            icon.name: "preferences-other"
            page: Qt.resolvedUrl("settings/GeneralSettingsPage.qml")
        },
        KirigamiSettings.SettingAction {
            actionName: "tmdb"
            text: i18nc("@title:tab settings page", "TMDB (Discover)")
            icon.name: "applications-multimedia"
            page: Qt.resolvedUrl("settings/TmdbSettingsPage.qml")
        },
        KirigamiSettings.SettingAction {
            actionName: "realdebrid"
            text: i18nc("@title:tab settings page", "Real-Debrid")
            icon.name: "network-server"
            page: Qt.resolvedUrl("settings/RealDebridSettingsPage.qml")
        },
        KirigamiSettings.SettingAction {
            actionName: "filters"
            text: i18nc("@title:tab settings page", "Filters")
            icon.name: "view-filter"
            page: Qt.resolvedUrl("settings/FiltersSettingsPage.qml")
        },
        KirigamiSettings.SettingAction {
            actionName: "player"
            text: i18nc("@title:tab settings page", "Player")
            icon.name: "media-playback-start"
            page: Qt.resolvedUrl("settings/PlayerSettingsPage.qml")
        },
        KirigamiSettings.SettingAction {
            actionName: "subtitles"
            text: i18nc("@title:tab settings page", "Subtitles")
            icon.name: "media-view-subtitles-symbolic"
            page: Qt.resolvedUrl("settings/SubtitlesSettingsPage.qml")
        },
        KirigamiSettings.SettingAction {
            actionName: "appearance"
            text: i18nc("@title:tab settings page", "Appearance")
            icon.name: "preferences-desktop-theme"
            page: Qt.resolvedUrl("settings/AppearanceSettingsPage.qml")
        }
    ]

    Component.onCompleted: {
        // Drop the requested category once consumed so a later
        // open without an explicit category lands on `general`.
        if (settingsVm.initialCategory.length > 0) {
            Qt.callLater(() => settingsVm.clearInitialCategory());
        }
    }
}
