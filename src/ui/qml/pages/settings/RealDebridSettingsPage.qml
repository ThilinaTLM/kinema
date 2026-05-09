// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

FormCard.FormCardPage {
    title: i18nc("@title:tab settings page", "Real-Debrid")

    readonly property var vm: settingsVm.realDebrid

    FormCard.FormHeader {
        title: i18nc("@title:group real-debrid token settings", "API token")
    }
    FormCard.FormCard {
        FormCard.FormSwitchDelegate {
            text: i18nc("@option:check real-debrid settings",
                "Use Real-Debrid for stream resolution")
            description: i18nc("@info rd settings toggle",
                "Turning this off keeps your token in the system keyring, "
                + "but Kinema stops requesting Real-Debrid direct URLs so "
                + "you can test built-in torrent streaming.")
            checked: vm.enabled
            onToggled: vm.enabled = checked
        }
        FormCard.FormTextDelegate {
            text: i18nc("@info rd settings intro",
                "Real-Debrid API token")
            description: i18nc("@info rd settings intro",
                "Get your token from real-debrid.com/apitoken. The token "
                + "is stored in your system keyring — never on disk in "
                + "plaintext.")
        }
        FormCard.FormPasswordFieldDelegate {
            id: tokenField
            label: i18nc("@label:textbox", "API token")
            placeholderText: i18nc("@info:placeholder",
                "Real-Debrid API token")
            text: vm.token
            onTextChanged: vm.token = text
            statusMessage: vm.statusMessage
            status: vm.statusKind
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button", "Test connection")
            icon.source: AppIcons.url("search-check")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: !vm.busy && tokenField.text.length > 0
            onClicked: vm.testConnection()
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button", "Save token")
            icon.source: AppIcons.url("save")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: !vm.busy && tokenField.text.length > 0
            onClicked: vm.save()
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button", "Remove token")
            icon.source: AppIcons.url("trash-2")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: !vm.busy && vm.tokenSaved
            onClicked: vm.remove()
        }
    }
}
