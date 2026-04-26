// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

FormCard.FormCardPage {
    title: i18nc("@title:tab settings page", "Appearance")

    readonly property var vm: settingsVm.appearance

    FormCard.FormHeader {
        title: i18nc("@title:group window behaviour", "Window")
    }
    FormCard.FormCard {
        FormCard.FormSwitchDelegate {
            text: i18nc("@option:check", "Close to system tray")
            description: vm.trayAvailable
                ? i18nc("@info form delegate hint",
                    "Closing the main window hides Kinema to the tray "
                    + "instead of quitting.")
                : i18nc("@info form delegate hint, no tray",
                    "Your desktop does not expose a system tray.")
            checked: vm.closeToTray
            enabled: vm.trayAvailable
            onToggled: vm.closeToTray = checked
        }
    }

    FormCard.FormHeader {
        title: i18nc("@title:group appearance theme", "Theme")
    }
    FormCard.FormCard {
        FormCard.FormTextDelegate {
            text: i18nc("@info appearance follows plasma",
                "Kinema follows your Plasma colour scheme, font, and "
                + "icon theme.")
            description: i18nc("@info appearance follows plasma",
                "Change them from System Settings → Appearance to "
                + "see the effect across the app.")
        }
    }
}
