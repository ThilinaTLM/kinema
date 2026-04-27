// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

FormCard.FormCardPage {
    title: i18nc("@title:tab settings page", "Subtitles")

    readonly property var vm: settingsVm.subtitles

    FormCard.FormHeader {
        title: i18nc("@title:group OpenSubtitles credentials",
            "OpenSubtitles account")
    }
    FormCard.FormCard {
        FormCard.FormPasswordFieldDelegate {
            id: apiKeyField
            label: i18nc("@label:textbox", "API key")
            placeholderText: i18nc("@info:placeholder", "API key")
            text: vm.apiKey
            onTextChanged: vm.apiKey = text
        }
        FormCard.FormTextFieldDelegate {
            id: usernameField
            label: i18nc("@label:textbox", "Username")
            placeholderText: i18nc("@info:placeholder", "Username")
            text: vm.username
            onTextChanged: vm.username = text
        }
        FormCard.FormPasswordFieldDelegate {
            id: passwordField
            label: i18nc("@label:textbox", "Password")
            placeholderText: i18nc("@info:placeholder", "Password")
            text: vm.password
            onTextChanged: vm.password = text
            statusMessage: vm.statusMessage
            status: vm.statusKind
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button", "Test connection")
            icon.name: "network-connect"
            enabled: !vm.busy
                && apiKeyField.text.length > 0
                && usernameField.text.length > 0
                && passwordField.text.length > 0
            onClicked: vm.testConnection()
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button", "Save credentials")
            icon.name: "document-save"
            enabled: !vm.busy
                && apiKeyField.text.length > 0
                && usernameField.text.length > 0
                && passwordField.text.length > 0
            onClicked: vm.saveCredentials()
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button", "Remove credentials")
            icon.name: "edit-delete"
            enabled: !vm.busy && vm.credentialsSaved
            onClicked: vm.removeCredentials()
        }
    }

    FormCard.FormHeader {
        title: i18nc("@title:group", "Preferred languages")
    }
    FormCard.FormCard {
        // Selected languages, in display order. Reorder + remove
        // controls live on each row so the page stays a single
        // FormCard scroll.
        Repeater {
            model: vm.preferredLanguages
            delegate: FormCard.AbstractFormDelegate {
                required property int index
                required property var modelData
                contentItem: RowLayout {
                    spacing: Theme.inlineSpacing
                    QQC2.Label {
                        Layout.fillWidth: true
                        text: `${vm.languageDisplayName(modelData)} (${modelData.toUpperCase()})`
                        elide: Text.ElideRight
                    }
                    QQC2.ToolButton {
                        icon.name: "go-up"
                        enabled: index > 0
                        onClicked: vm.moveLanguage(index, index - 1)
                    }
                    QQC2.ToolButton {
                        icon.name: "go-down"
                        enabled: index < vm.preferredLanguages.length - 1
                        onClicked: vm.moveLanguage(index, index + 1)
                    }
                    QQC2.ToolButton {
                        icon.name: "list-remove"
                        onClicked: vm.removeLanguageAt(index)
                    }
                }
            }
        }
        FormCard.FormDelegateSeparator {}
        FormCard.AbstractFormDelegate {
            id: addRow
            property string pendingCode: ""
            contentItem: RowLayout {
                spacing: Theme.inlineSpacing
                QQC2.ComboBox {
                    Layout.fillWidth: true
                    model: vm.commonLanguages
                    textRole: "display"
                    valueRole: "code"
                    onActivated: addRow.pendingCode = currentValue
                    Component.onCompleted: addRow.pendingCode = currentValue
                }
                QQC2.Button {
                    text: i18nc("@action:button add a language", "Add")
                    icon.name: "list-add"
                    onClicked: {
                        if (addRow.pendingCode.length > 0) {
                            vm.addLanguage(addRow.pendingCode);
                        }
                    }
                }
            }
        }
    }

    FormCard.FormHeader {
        title: i18nc("@title:group subtitles default filter modes",
            "Default filters")
    }
    FormCard.FormCard {
        FormCard.FormComboBoxDelegate {
            text: i18nc("@label:listbox", "Hearing impaired")
            model: [
                { value: "off",     label: i18nc("@item:inlistbox", "Off") },
                { value: "include", label: i18nc("@item:inlistbox", "Include") },
                { value: "only",    label: i18nc("@item:inlistbox", "Only") }
            ]
            textRole: "label"
            valueRole: "value"
            currentIndex: {
                switch (vm.hearingImpaired) {
                case "include": return 1;
                case "only":    return 2;
                default:        return 0;
                }
            }
            onActivated: vm.hearingImpaired = currentValue
        }
        FormCard.FormComboBoxDelegate {
            text: i18nc("@label:listbox", "Foreign parts only")
            model: [
                { value: "off",     label: i18nc("@item:inlistbox", "Off") },
                { value: "include", label: i18nc("@item:inlistbox", "Include") },
                { value: "only",    label: i18nc("@item:inlistbox", "Only") }
            ]
            textRole: "label"
            valueRole: "value"
            currentIndex: {
                switch (vm.foreignPartsOnly) {
                case "include": return 1;
                case "only":    return 2;
                default:        return 0;
                }
            }
            onActivated: vm.foreignPartsOnly = currentValue
        }
    }

    FormCard.FormHeader {
        title: i18nc("@title:group", "Cache")
    }
    FormCard.FormCard {
        FormCard.FormSpinBoxDelegate {
            label: i18nc("@label:spinbox subtitle disk budget",
                "Disk budget (MB)")
            value: vm.subtitleBudgetMb
            from: 1
            to: 10000
            onValueChanged: vm.subtitleBudgetMb = value
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button", "Clear subtitle cache")
            icon.name: "edit-clear"
            onClicked: vm.clearCache()
        }
    }
}
