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
// sort, base URL, \u2026) lives in dedicated sections below.
//
// Mirrors the Debrid settings page: a radio at the top to pick the
// active option, then one FormCard section per registered indexer
// for its specific knobs and a Test button.
FormCard.FormCardPage {
    title: i18nc("@title:tab settings page", "Indexers")

    readonly property var vm: settingsVm.indexers
    readonly property var torrentioSection: vm.torrentio
    readonly property var peerflixSection: vm.peerflix

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
            // IndexerKind::Peerflix == 2
            text: i18nc("@option:radio indexer", "Peerflix")
            description: i18nc("@info active indexer radio",
                "Peerflix's free public addon. Different scraper pool "
                + "than Torrentio \u2014 useful as a fallback or for "
                + "non-Torrentio releases. No configuration required.")
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

    // ---- Peerflix section ----------------------------------------------
    FormCard.FormHeader {
        title: i18nc("@title:group peerflix settings",
            "Peerflix")
    }
    FormCard.FormCard {
        FormCard.FormTextDelegate {
            description: i18nc("@info peerflix settings intro",
                "Peerflix is a free, zero-configuration Stremio addon "
                + "(peerflix.mov) that scrapes Bitsearch, YTS, "
                + "Popcorntime, Dontorrent, Mejortorrent and Wolfmax "
                + "4K. Returns magnet links directly. Many releases "
                + "are Spanish-language \u2014 enable Streams \u2192 "
                + "Hide variants \u2192 Non-English to hide them.")
        }
        FormCard.FormTextFieldDelegate {
            id: peerflixBaseField
            label: i18nc("@label:textbox", "Base URL")
            placeholderText: peerflixSection.defaultBaseUrl
            text: peerflixSection.baseUrl
            onEditingFinished: peerflixSection.baseUrl = text
            statusMessage: peerflixSection.statusMessage
            status: peerflixSection.statusKind
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button",
                "Reset to public host")
            icon.source: AppIcons.url("eraser")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: !peerflixSection.busy
                && peerflixSection.baseUrl !== peerflixSection.defaultBaseUrl
            onClicked: {
                peerflixSection.resetBaseUrl();
                peerflixBaseField.text = peerflixSection.baseUrl;
            }
        }
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button", "Test connection")
            icon.source: AppIcons.url("search-check")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: !peerflixSection.busy
            onClicked: peerflixSection.testConnection()
        }
    }
}
