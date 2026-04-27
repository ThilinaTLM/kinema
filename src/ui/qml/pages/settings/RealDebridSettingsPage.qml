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
            icon.name: "network-connect"
            enabled: !vm.busy && tokenField.text.length > 0
            onClicked: vm.testConnection()
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button", "Save token")
            icon.name: "document-save"
            enabled: !vm.busy && tokenField.text.length > 0
            onClicked: vm.save()
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button", "Remove token")
            icon.name: "edit-delete"
            enabled: !vm.busy && vm.tokenSaved
            onClicked: vm.remove()
        }
    }
}
