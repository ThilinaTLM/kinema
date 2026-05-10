// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

// Unified Debrid settings page.
//
// Both Real-Debrid and AllDebrid credentials live independently in the
// system keyring. The "Active provider" radio at the top picks which
// one the unified downloader routes new sessions through; the other
// provider's credential stays saved but is only reachable through the
// per-row override menu (or row-level resume of a download persisted
// against the other provider).
//
// Each provider section is a self-contained FormCard with its own
// API-key field, Test, Save and Remove buttons \u2014 matching the layout
// of the legacy Real-Debrid page so existing users can reach the same
// affordances without learning a new flow.
FormCard.FormCardPage {
    title: i18nc("@title:tab settings page", "Debrid")

    readonly property var vm: settingsVm.debrid
    readonly property var rdSection: vm.realDebrid
    readonly property var adSection: vm.allDebrid

    // ---- Active provider radio -----------------------------------------
    FormCard.FormHeader {
        title: i18nc("@title:group debrid provider radio",
            "Active provider")
    }
    FormCard.FormCard {
        FormCard.FormRadioDelegate {
            text: i18nc("@option:radio debrid provider", "None")
            description: i18nc("@info debrid provider radio",
                "Use Kinema's built-in libtorrent backend for every "
                + "stream. Saved API keys stay in the keyring.")
            checked: vm.activeProvider === 0
            onToggled: if (checked) vm.activeProvider = 0
        }
        FormCard.FormRadioDelegate {
            text: i18nc("@option:radio debrid provider", "Real-Debrid")
            description: i18nc("@info debrid provider radio",
                "Route streams through Real-Debrid. Requires a saved "
                + "API token below.")
            checked: vm.activeProvider === 1
            onToggled: if (checked) vm.activeProvider = 1
        }
        FormCard.FormRadioDelegate {
            text: i18nc("@option:radio debrid provider", "AllDebrid")
            description: i18nc("@info debrid provider radio",
                "Route streams through AllDebrid. Requires a saved "
                + "API key below.")
            checked: vm.activeProvider === 2
            onToggled: if (checked) vm.activeProvider = 2
        }
    }

    // ---- Real-Debrid section -------------------------------------------
    FormCard.FormHeader {
        title: i18nc("@title:group real-debrid token settings",
            "Real-Debrid")
    }
    FormCard.FormCard {
        FormCard.FormTextDelegate {
            text: i18nc("@info rd settings intro",
                "Real-Debrid API token")
            description: i18nc("@info rd settings intro",
                "Get your token from real-debrid.com/apitoken. The token "
                + "is stored in your system keyring \u2014 never on disk in "
                + "plaintext.")
        }
        FormCard.FormPasswordFieldDelegate {
            id: rdTokenField
            label: i18nc("@label:textbox", "API token")
            placeholderText: i18nc("@info:placeholder",
                "Real-Debrid API token")
            text: rdSection.token
            onTextChanged: rdSection.token = text
            statusMessage: rdSection.statusMessage
            status: rdSection.statusKind
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button", "Test connection")
            icon.source: AppIcons.url("search-check")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: !rdSection.busy && rdTokenField.text.length > 0
            onClicked: rdSection.testConnection()
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button", "Save token")
            icon.source: AppIcons.url("save")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: !rdSection.busy && rdTokenField.text.length > 0
            onClicked: rdSection.save()
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button", "Remove token")
            icon.source: AppIcons.url("trash-2")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: !rdSection.busy && rdSection.tokenSaved
            onClicked: rdSection.remove()
        }
    }

    // ---- AllDebrid section ---------------------------------------------
    FormCard.FormHeader {
        title: i18nc("@title:group alldebrid apikey settings",
            "AllDebrid")
    }
    FormCard.FormCard {
        FormCard.FormTextDelegate {
            text: i18nc("@info ad settings intro",
                "AllDebrid API key")
            description: i18nc("@info ad settings intro",
                "Get your API key from alldebrid.com/apikeys. The key "
                + "is stored in your system keyring \u2014 never on disk in "
                + "plaintext.")
        }
        FormCard.FormPasswordFieldDelegate {
            id: adKeyField
            label: i18nc("@label:textbox", "API key")
            placeholderText: i18nc("@info:placeholder",
                "AllDebrid API key")
            text: adSection.apiKey
            onTextChanged: adSection.apiKey = text
            statusMessage: adSection.statusMessage
            status: adSection.statusKind
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button", "Test connection")
            icon.source: AppIcons.url("search-check")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: !adSection.busy && adKeyField.text.length > 0
            onClicked: adSection.testConnection()
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button", "Save API key")
            icon.source: AppIcons.url("save")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: !adSection.busy && adKeyField.text.length > 0
            onClicked: adSection.save()
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button", "Remove API key")
            icon.source: AppIcons.url("trash-2")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: !adSection.busy && adSection.apiKeySaved
            onClicked: adSection.remove()
        }
    }
}
