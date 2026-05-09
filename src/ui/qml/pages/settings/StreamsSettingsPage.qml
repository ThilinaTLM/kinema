// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

FormCard.FormCardPage {
    title: i18nc("@title:tab settings page", "Streams")

    readonly property var vm: settingsVm.streams

    FormCard.FormHeader {
        title: i18nc("@title:group", "Defaults")
    }
    FormCard.FormCard {
        FormCard.FormComboBoxDelegate {
            text: i18nc("@label:listbox", "Default torrent sort")
            model: [
                i18nc("@item:inlistbox sort mode", "Seeders"),
                i18nc("@item:inlistbox sort mode", "Size"),
                i18nc("@item:inlistbox sort mode", "Quality & Size")
            ]
            currentIndex: vm.defaultTorrentioSort
            onActivated: vm.defaultTorrentioSort = currentIndex
            description: i18nc("@info form delegate hint",
                "Applied to every Torrentio query. You can still re-sort "
                + "any column from the streams list header.")
        }
    }

    FormCard.FormHeader {
        title: i18nc("@title:group", "Hide resolutions")
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
        title: i18nc("@title:group", "Hide variants")
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
        title: i18nc("@title:group streams keyword blocklist",
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
                "Case-insensitive substring match against the stream "
                + "name and description line.")
        }
    }
}
