// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

FormCard.FormCardPage {
    title: i18nc("@title:tab settings page", "Filters")

    readonly property var vm: settingsVm.filters

    FormCard.FormHeader {
        title: i18nc("@title:group", "Exclude resolutions")
    }
    FormCard.FormCard {
        Repeater {
            model: vm.resolutionOptions
            delegate: FormCard.FormCheckDelegate {
                required property var modelData
                required property int index
                text: modelData.label
                checked: vm.resolutionExcluded(modelData.token)
                onToggled: vm.toggleResolution(modelData.token, checked)
            }
        }
    }

    FormCard.FormHeader {
        title: i18nc("@title:group", "Exclude variants")
    }
    FormCard.FormCard {
        Repeater {
            model: vm.categoryOptions
            delegate: FormCard.FormCheckDelegate {
                required property var modelData
                required property int index
                text: modelData.label
                checked: vm.categoryExcluded(modelData.token)
                onToggled: vm.toggleCategory(modelData.token, checked)
            }
        }
    }

    FormCard.FormHeader {
        title: i18nc("@title:group filters keyword blocklist",
            "Keyword blocklist")
    }
    FormCard.FormCard {
        FormCard.FormTextAreaDelegate {
            label: i18nc("@label keyword blocklist edit area",
                "Keywords (one per line)")
            placeholderText: i18nc("@info:placeholder",
                "e.g. HINDI\nYIFY\nNSFW")
            text: vm.blocklistText
            onTextChanged: vm.blocklistText = text
        }
        FormCard.FormTextDelegate {
            description: i18nc("@info form delegate hint",
                "Case-insensitive substring match against the release "
                + "name and description line.")
        }
    }
}
