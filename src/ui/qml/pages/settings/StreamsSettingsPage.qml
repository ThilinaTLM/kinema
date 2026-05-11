// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

// Indexer-agnostic stream filters. Applied client-side after the active
// indexer has answered, so the same blocklist / exclusion settings
// work uniformly across Torrentio and Peerflix. Indexer-specific
// knobs (default sort, base URL) live on the Indexers page.
FormCard.FormCardPage {
    title: i18nc("@title:tab settings page", "Streams")

    readonly property var vm: settingsVm.streams

    FormCard.FormHeader {
        title: i18nc("@title:group streams page intro",
            "Filters")
    }
    FormCard.FormCard {
        FormCard.FormTextDelegate {
            description: i18nc("@info streams settings intro",
                "Hide unwanted streams from every indexer. Torrentio "
                + "also receives these exclusions in its URL so it can "
                + "trim the payload server-side; Peerflix applies them "
                + "client-side only.")
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
