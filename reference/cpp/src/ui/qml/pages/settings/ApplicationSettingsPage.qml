// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

FormCard.FormCardPage {
    title: i18nc("@title:tab settings page", "Application")

    readonly property var vm: settingsVm.general

    FormCard.FormHeader {
        title: i18nc("@title:group", "Search defaults")
    }
    FormCard.FormCard {
        FormCard.FormComboBoxDelegate {
            text: i18nc("@label:listbox", "Default search kind")
            model: [
                i18nc("@item:inlistbox", "Movie"),
                i18nc("@item:inlistbox", "Series")
            ]
            currentIndex: vm.defaultSearchKind
            onActivated: vm.defaultSearchKind = currentIndex
        }
    }

    FormCard.FormHeader {
        title: i18nc("@title:group window behaviour", "Window")
    }
    FormCard.FormCard {
        FormCard.FormSwitchDelegate {
            text: i18nc("@option:check", "Close to system tray")
            description: vm.trayAvailable
                ? i18nc("@info form delegate hint",
                    "Closing the main window hides Kinema to the tray "
                    + "instead of quitting. Use the tray icon or Ctrl+Q "
                    + "to quit.")
                : i18nc("@info form delegate hint, no tray",
                    "Your desktop does not expose a system tray, so "
                    + "close-to-tray is unavailable.")
            checked: vm.closeToTray
            enabled: vm.trayAvailable
            onToggled: vm.closeToTray = checked
        }
    }
}
