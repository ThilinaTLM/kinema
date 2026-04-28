// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Dialogs as QQDialogs
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Body-only subtitles results surface.
//
// Filter chrome (release-name SearchField, Languages multi-select menu,
// HI / FPO sub-menus) lives on the parent `SubtitlesPage` header; this
// component is responsible only for the status line, the state-switched
// results region (idle / loading / error / empty / not-configured /
// ready), and the footer toolbar (Open file… / primary action).
Item {
    id: panel

    /// Subtitles VM. Defaults to the shell's `subtitlesVm` context
    /// property; injected explicitly for clarity at call sites.
    property var vm: subtitlesVm

    QQDialogs.FileDialog {
        id: localFileDialog
        title: i18nc("@title:window", "Open subtitle file")
        nameFilters: [
            i18nc("@info file dialog filter",
                "Subtitles (*.srt *.vtt *.ass *.ssa *.sub)")
        ]
        onAccepted: panel.vm.localFileResolved(selectedFile)
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Theme.pageMargin
        spacing: Theme.groupSpacing

        QQC2.Label {
            Layout.fillWidth: true
            text: panel.vm ? panel.vm.statusLine : ""
            visible: text.length > 0
            opacity: 0.75
            font.pointSize: Kirigami.Theme.smallFont.pointSize
            wrapMode: Text.Wrap
        }

        Loader {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: Kirigami.Units.gridUnit * 12
            sourceComponent: {
                if (!panel.vm) {
                    return idleComp;
                }
                switch (panel.vm.state) {
                case "loading":       return loadingComp;
                case "error":         return errorComp;
                case "empty":         return emptyComp;
                case "notconfigured": return notConfiguredComp;
                case "ready":         return resultsComp;
                default:               return idleComp;
                }
            }
        }

        QQC2.ToolBar {
            Layout.fillWidth: true
            contentItem: RowLayout {
                spacing: Theme.inlineSpacing

                QQC2.Button {
                    text: i18nc("@action:button subtitles open local file",
                        "Open file…")
                    icon.name: "document-open"
                    enabled: !!panel.vm
                    onClicked: localFileDialog.open()
                }
                Item { Layout.fillWidth: true }
                QQC2.Button {
                    text: panel.vm ? panel.vm.primaryActionText : ""
                    icon.name: panel.vm ? panel.vm.primaryActionIcon : ""
                    enabled: panel.vm && panel.vm.primaryActionEnabled
                    highlighted: true
                    onClicked: panel.vm.runPrimaryAction()
                }
            }
        }
    }

    Component {
        id: idleComp
        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: Math.min(parent.width - Theme.pageWideMargin * 2,
                Theme.placeholderMaxWidth)
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
            width: Math.min(parent.width - Theme.pageWideMargin * 2,
                Theme.placeholderMaxWidth)
            icon.name: "dialog-error"
            text: i18nc("@info subtitles error state", "Search failed.")
            explanation: panel.vm ? panel.vm.errorText : ""
            helpfulAction: Kirigami.Action {
                text: i18nc("@action retry subtitles search", "Retry")
                icon.name: "view-refresh"
                onTriggered: panel.vm.runSearch()
            }
        }
    }
    Component {
        id: emptyComp
        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: Math.min(parent.width - Theme.pageWideMargin * 2,
                Theme.placeholderMaxWidth)
            icon.name: "edit-find"
            text: i18nc("@info subtitles empty state", "No subtitles found.")
            explanation: i18nc("@info subtitles empty state explanation",
                "Try widening the languages or removing the release filter.")
        }
    }
    Component {
        id: notConfiguredComp
        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: Math.min(parent.width - Theme.pageWideMargin * 2,
                Theme.placeholderMaxWidth)
            icon.name: "configure"
            text: i18nc("@info subtitles not configured state",
                "OpenSubtitles isn't configured yet.")
            explanation: i18nc("@info subtitles not configured state explanation",
                "Add credentials in Settings to search.")
            helpfulAction: Kirigami.Action {
                text: i18nc("@action open settings from subtitles page",
                    "Open settings…")
                icon.name: "configure"
                onTriggered: panel.vm.openSettings()
            }
        }
    }
    Component {
        id: resultsComp
        ListView {
            clip: true
            spacing: 0
            model: panel.vm ? panel.vm.results : null
            currentIndex: panel.vm ? panel.vm.selectedRow : -1
            boundsBehavior: Flickable.StopAtBounds
            delegate: SubtitleResultRow {
                selected: index === panel.vm.selectedRow
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
                onClicked: panel.vm.selectedRow = index
                onActivated: panel.vm.runPrimaryAction()
            }
        }
    }
}
