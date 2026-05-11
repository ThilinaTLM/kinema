// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

// Stream indexer settings page.
//
// "Indexer" is Kinema's term for the service that, given a Stremio
// stream id, returns candidate streams. The active indexer radio
// picks which one detail pages query; per-indexer config (default
// sort, base URL, MediaFusion manifest token, \u2026) lives in
// dedicated sections below.
//
// Mirrors the Debrid settings page: a radio at the top to pick the
// active option, then one FormCard section per registered indexer
// for its specific knobs and a Test button.
FormCard.FormCardPage {
    title: i18nc("@title:tab settings page", "Indexers")

    readonly property var vm: settingsVm.indexers
    readonly property var torrentioSection: vm.torrentio
    readonly property var mediaFusionSection: vm.mediaFusion

    // ---- Active indexer radio ------------------------------------------
    FormCard.FormHeader {
        title: i18nc("@title:group active indexer radio",
            "Active indexer")
    }
    FormCard.FormCard {
        FormCard.FormRadioDelegate {
            // IndexerKind::Torrentio == 1
            text: i18nc("@option:radio indexer", "Torrentio")
            description: i18nc("@info active indexer radio",
                "Torrentio's public Stremio addon. Works without setup; "
                + "the URL config below is optional.")
            checked: vm.activeIndexer === 1
            onToggled: if (checked) vm.activeIndexer = 1
        }
        FormCard.FormRadioDelegate {
            // IndexerKind::MediaFusion == 2
            text: i18nc("@option:radio indexer", "MediaFusion")
            description: i18nc("@info active indexer radio",
                "MediaFusion aggregates many indexers and typically "
                + "returns more streams than Torrentio. Requires a "
                + "personalised manifest URL \u2014 see the section "
                + "below.")
            checked: vm.activeIndexer === 2
            onToggled: if (checked) vm.activeIndexer = 2
        }
    }

    // ---- Torrentio section ---------------------------------------------
    FormCard.FormHeader {
        title: i18nc("@title:group torrentio settings",
            "Torrentio")
    }
    FormCard.FormCard {
        FormCard.FormComboBoxDelegate {
            text: i18nc("@label:listbox", "Default sort")
            model: [
                i18nc("@item:inlistbox sort mode", "Seeders"),
                i18nc("@item:inlistbox sort mode", "Size"),
                i18nc("@item:inlistbox sort mode", "Quality & Size")
            ]
            currentIndex: torrentioSection.defaultSort
            onActivated: torrentioSection.defaultSort = currentIndex
            description: i18nc("@info form delegate hint",
                "Encoded into the Torrentio URL. You can still re-sort "
                + "from the streams list header at any time.")
        }
        FormCard.FormTextFieldDelegate {
            id: torrentioBaseField
            label: i18nc("@label:textbox", "Base URL")
            placeholderText: torrentioSection.defaultBaseUrl
            text: torrentioSection.baseUrl
            onEditingFinished: torrentioSection.baseUrl = text
            statusMessage: torrentioSection.statusMessage
            status: torrentioSection.statusKind
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button",
                "Reset to public host")
            icon.source: AppIcons.url("eraser")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: !torrentioSection.busy
                && torrentioSection.baseUrl !== torrentioSection.defaultBaseUrl
            onClicked: {
                torrentioSection.resetBaseUrl();
                torrentioBaseField.text = torrentioSection.baseUrl;
            }
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button", "Test connection")
            icon.source: AppIcons.url("search-check")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: !torrentioSection.busy
            onClicked: torrentioSection.testConnection()
        }
    }

    // ---- MediaFusion section -------------------------------------------
    FormCard.FormHeader {
        title: i18nc("@title:group mediafusion settings",
            "MediaFusion")
    }
    FormCard.FormCard {
        FormCard.FormTextDelegate {
            text: i18nc("@info mediafusion settings intro",
                "MediaFusion manifest URL (required)")
            description: i18nc("@info mediafusion settings intro",
                "Open https://mediafusion.elfhosted.com/app/configure "
                + "(or your self-hosted instance), pick indexers, a "
                + "streaming provider (Real-Debrid / AllDebrid / \u2026) "
                + "and filters, then paste the resulting manifest URL "
                + "(ending in \u201c/manifest.json\u201d) below. The "
                + "public host does not allow unconfigured access.")
        }
        FormCard.FormTextFieldDelegate {
            id: mfManifestField
            label: i18nc("@label:textbox", "Manifest URL")
            placeholderText: i18nc("@info:placeholder",
                "https://mediafusion.elfhosted.com/D-\u2026/manifest.json")
            text: mediaFusionSection.manifestUrl
            onTextChanged: mediaFusionSection.manifestUrl = text
            statusMessage: mediaFusionSection.statusMessage
            status: mediaFusionSection.statusKind
        }
        FormCard.FormTextDelegate {
            text: i18nc("@label mediafusion saved host",
                "Saved host")
            description: mediaFusionSection.baseUrl
                + (mediaFusionSection.tokenPresent
                    ? i18nc("@info mediafusion token state",
                        " \u2014 personalised token present")
                    : i18nc("@info mediafusion token state",
                        " \u2014 no token saved (default pool only)"))
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button", "Test connection")
            icon.source: AppIcons.url("search-check")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: !mediaFusionSection.busy
            onClicked: mediaFusionSection.testConnection()
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button", "Save manifest URL")
            icon.source: AppIcons.url("save")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: !mediaFusionSection.busy
                && mfManifestField.text.trim().length > 0
            onClicked: mediaFusionSection.save()
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button", "Clear")
            icon.source: AppIcons.url("trash-2")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: !mediaFusionSection.busy
                && mediaFusionSection.configured
            onClicked: mediaFusionSection.clear()
        }
    }
}
