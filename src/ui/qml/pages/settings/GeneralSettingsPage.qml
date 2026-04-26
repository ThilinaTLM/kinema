// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

FormCard.FormCardPage {
    title: i18nc("@title:tab settings page", "General")

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
            currentIndex: settingsVm.general.defaultSearchKind
            onActivated: settingsVm.general.defaultSearchKind = currentIndex
        }
        FormCard.FormDelegateSeparator {}
        FormCard.FormComboBoxDelegate {
            text: i18nc("@label:listbox", "Default torrent sort")
            model: [
                i18nc("@item:inlistbox sort mode", "Seeders"),
                i18nc("@item:inlistbox sort mode", "Size"),
                i18nc("@item:inlistbox sort mode", "Quality & Size")
            ]
            currentIndex: settingsVm.general.defaultTorrentioSort
            onActivated: settingsVm.general.defaultTorrentioSort = currentIndex
            description: i18nc("@info form delegate hint",
                "Applied to every Torrentio query. You can still re-sort "
                + "any column from the streams list header.")
        }
    }

    FormCard.FormHeader {
        title: i18nc("@title:group window behaviour", "Window")
    }
    FormCard.FormCard {
        FormCard.FormSwitchDelegate {
            text: i18nc("@option:check", "Close to system tray")
            description: settingsVm.general.trayAvailable
                ? i18nc("@info form delegate hint",
                    "Closing the main window hides Kinema to the tray "
                    + "instead of quitting. Use the tray icon or Ctrl+Q "
                    + "to quit.")
                : i18nc("@info form delegate hint, no tray",
                    "Your desktop does not expose a system tray, so "
                    + "close-to-tray is unavailable.")
            checked: settingsVm.general.closeToTray
            enabled: settingsVm.general.trayAvailable
            onToggled: settingsVm.general.closeToTray = checked
        }
    }
}
