// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtQuick.Dialogs as QQDialogs
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

// Subtitles search + download page. Pushed on top of the current
// nav stack from a detail page's "Subtitles…" header action, from
// a stream-row context menu, or from the embedded player chrome's
// `SubtitlePicker → Download…` entry. Replaces the modal
// `SubtitlesDialog` widget — same controller, same flow, native
// Kirigami chrome.
Kirigami.ScrollablePage {
    id: page

    objectName: "subtitles"
    title: i18nc("@title:window subtitles search page",
        "Subtitles for %1", subtitlesVm.contextTitle)

    actions: [
        Kirigami.Action {
            icon.name: "configure"
            text: i18nc("@action subtitles page header, open settings",
                "Open settings…")
            onTriggered: subtitlesVm.openSettings()
        }
    ]

    QQDialogs.FileDialog {
        id: localFileDialog
        title: i18nc("@title:window", "Open subtitle file")
        nameFilters: [
            i18nc("@info file dialog filter",
                "Subtitles (*.srt *.vtt *.ass *.ssa *.sub)")
        ]
        onAccepted: subtitlesVm.localFileResolved(selectedFile)
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing
        width: page.width

        FormCard.FormHeader {
            title: i18nc("@title:group subtitles filter card", "Filters")
        }
        FormCard.FormCard {
            LanguagePicker {
                Layout.fillWidth: true
                languages: subtitlesVm.languages
                options: subtitlesVm.commonLanguages
                onLanguagesPicked: codes => subtitlesVm.languages = codes
            }
            FormCard.FormDelegateSeparator {}
            FormCard.AbstractFormDelegate {
                background: null
                contentItem: ThreeStateChips {
                    label: i18nc("@label subtitles filter row", "Hearing impaired")
                    value: subtitlesVm.hi
                    onValuePicked: v => subtitlesVm.hi = v
                }
            }
            FormCard.AbstractFormDelegate {
                background: null
                contentItem: ThreeStateChips {
                    label: i18nc("@label subtitles filter row", "Foreign parts only")
                    value: subtitlesVm.fpo
                    onValuePicked: v => subtitlesVm.fpo = v
                }
            }
            FormCard.FormDelegateSeparator {}
            FormCard.FormTextFieldDelegate {
                label: i18nc("@label:textbox subtitles filter row",
                    "Release name (optional)")
                text: subtitlesVm.release
                placeholderText: i18nc("@info:placeholder subtitles release filter",
                    "e.g. 1080p, BluRay, REMUX")
                onTextChanged: subtitlesVm.release = text
                onAccepted: subtitlesVm.runSearch()
            }
            FormCard.FormButtonDelegate {
                text: i18nc("@action:button", "Search")
                icon.name: "edit-find"
                onClicked: subtitlesVm.runSearch()
                enabled: subtitlesVm.state !== "notconfigured"
            }
        }

        // Status line — "%n results · Daily quota: N remaining".
        QQC2.Label {
            Layout.fillWidth: true
            Layout.leftMargin: Kirigami.Units.largeSpacing
            text: subtitlesVm.statusLine
            visible: text.length > 0
            opacity: 0.7
            font.pointSize: Kirigami.Theme.smallFont.pointSize
        }

        // Result surface. The state machine drives a switch over six
        // child placeholders / the list itself. ItemHandler-style
        // wrappers keep each branch self-contained so QML doesn't
        // have to recreate the heavy ListView delegate when state
        // briefly flips through "loading".
        Loader {
            Layout.fillWidth: true
            Layout.preferredHeight: subtitlesVm.state === "ready"
                ? Math.max(Kirigami.Units.gridUnit * 18, page.height * 0.5)
                : Kirigami.Units.gridUnit * 12
            sourceComponent: {
                switch (subtitlesVm.state) {
                case "loading":       return loadingComp;
                case "error":         return errorComp;
                case "empty":         return emptyComp;
                case "notconfigured": return notConfiguredComp;
                case "ready":         return resultsComp;
                default:              return idleComp;
                }
            }
        }

        Component {
            id: idleComp
            Kirigami.PlaceholderMessage {
                anchors.centerIn: parent
                width: parent.width - Kirigami.Units.largeSpacing * 4
                icon.name: "edit-find"
                text: i18nc("@info subtitles idle state",
                    "Pick a language and search.")
            }
        }
        Component {
            id: loadingComp
            Kirigami.LoadingPlaceholder {
                anchors.centerIn: parent
                text: i18nc("@info:status", "Searching subtitles…")
            }
        }
        Component {
            id: errorComp
            Kirigami.PlaceholderMessage {
                anchors.centerIn: parent
                width: parent.width - Kirigami.Units.largeSpacing * 4
                icon.name: "dialog-error"
                text: i18nc("@info subtitles error state",
                    "Search failed.")
                explanation: subtitlesVm.errorText
                helpfulAction: Kirigami.Action {
                    text: i18nc("@action retry subtitles search", "Retry")
                    icon.name: "view-refresh"
                    onTriggered: subtitlesVm.runSearch()
                }
            }
        }
        Component {
            id: emptyComp
            Kirigami.PlaceholderMessage {
                anchors.centerIn: parent
                width: parent.width - Kirigami.Units.largeSpacing * 4
                icon.name: "edit-find"
                text: i18nc("@info subtitles empty state",
                    "No subtitles found.")
                explanation: i18nc("@info subtitles empty state explanation",
                    "Try widening the languages or removing the release filter.")
            }
        }
        Component {
            id: notConfiguredComp
            Kirigami.PlaceholderMessage {
                anchors.centerIn: parent
                width: parent.width - Kirigami.Units.largeSpacing * 4
                icon.name: "configure"
                text: i18nc("@info subtitles not configured state",
                    "OpenSubtitles isn't configured yet.")
                explanation: i18nc("@info subtitles not configured state explanation",
                    "Add credentials in Settings to search.")
                helpfulAction: Kirigami.Action {
                    text: i18nc("@action open settings from subtitles page",
                        "Open settings…")
                    icon.name: "configure"
                    onTriggered: subtitlesVm.openSettings()
                }
            }
        }
        Component {
            id: resultsComp
            ListView {
                clip: true
                spacing: 0
                model: subtitlesVm.results
                currentIndex: subtitlesVm.selectedRow
                delegate: SubtitleResultRow {
                    selected: index === subtitlesVm.selectedRow
                    release: model.release
                    fileName: model.fileName
                    language: model.language
                    languageName: model.languageName
                    format: model.format
                    downloadCount: model.downloadCount
                    rating: model.rating
                    hearingImpaired: model.hearingImpaired
                    foreignPartsOnly: model.foreignPartsOnly
                    moviehashMatch: model.moviehashMatch
                    cached: model.cached
                    active: model.active
                    onClicked: subtitlesVm.selectedRow = index
                    onActivated: subtitlesVm.runPrimaryAction()
                }
            }
        }
    }

    footer: QQC2.ToolBar {
        contentItem: RowLayout {
            spacing: Kirigami.Units.smallSpacing

            QQC2.Button {
                text: i18nc("@action:button subtitles open local file",
                    "Open file…")
                icon.name: "document-open"
                onClicked: localFileDialog.open()
            }
            Item { Layout.fillWidth: true }
            QQC2.Button {
                text: i18nc("@action:button subtitles cancel", "Cancel")
                onClicked: subtitlesVm.closeRequested()
            }
            QQC2.Button {
                text: subtitlesVm.primaryActionText
                icon.name: subtitlesVm.primaryActionIcon
                enabled: subtitlesVm.primaryActionEnabled
                highlighted: true
                onClicked: subtitlesVm.runPrimaryAction()
            }
        }
    }
}
