// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

FormCard.FormCardPage {
    title: i18nc("@title:tab settings page", "TMDB (Discover)")

    readonly property var vm: settingsVm.tmdb

    FormCard.FormCard {
        FormCard.FormTextDelegate {
            text: i18nc("@info tmdb settings intro",
                "TMDB read-access token")
            description: i18nc("@info tmdb settings intro",
                "Discover uses a shared token that ships with Kinema. "
                + "Paste your own v4 read-access token to use your "
                + "account instead. The token is stored in your system "
                + "keyring — never on disk in plaintext.")
        }
        FormCard.FormPasswordFieldDelegate {
            id: tokenField
            label: i18nc("@label:textbox", "v4 Read Access Token")
            placeholderText: i18nc("@info:placeholder",
                "Leave empty to use the bundled token")
            text: vm.token
            onTextChanged: vm.token = text
            statusMessage: vm.statusMessage
            status: vm.statusKind
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button", "Test connection")
            icon.name: "network-connect"
            enabled: !vm.busy
                && (tokenField.text.length > 0
                    || vm.userTokenSaved
                    || vm.hasCompiledDefault)
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
            enabled: !vm.busy && vm.userTokenSaved
            onClicked: vm.remove()
        }
    }
}
