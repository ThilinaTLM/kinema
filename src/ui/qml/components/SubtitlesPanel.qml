// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Dialogs as QQDialogs
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.components as Components
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

// Reusable subtitles search/download surface. Detail pages embed this as an
// inline tab; SubtitlesPage.qml wraps the same component for player-initiated
// downloads and other standalone contexts.
Item {
    id: panel

    property var vm: subtitlesVm
    property bool showContextTitle: false
    property bool showCancelButton: false
    property bool compact: true

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

        Kirigami.Heading {
            Layout.fillWidth: true
            visible: panel.showContextTitle
                && panel.vm
                && panel.vm.contextTitle.length > 0
            level: 3
            text: panel.vm ? panel.vm.contextTitle : ""
            wrapMode: Text.Wrap
        }

        QQC2.ScrollView {
            id: filtersScroll
            Layout.fillWidth: true
            Layout.preferredHeight: panel.compact
                ? Math.min(contentHeight,
                    Kirigami.Units.gridUnit * 15)
                : contentHeight
            Layout.maximumHeight: panel.compact
                ? Kirigami.Units.gridUnit * 16
                : Number.POSITIVE_INFINITY
            clip: true
            contentWidth: availableWidth

            ColumnLayout {
                width: filtersScroll.availableWidth
                spacing: 0

                FormCard.FormHeader {
                    title: i18nc("@title:group subtitles filter card", "Filters")
                }
                FormCard.FormCard {
                    Layout.fillWidth: true

                    LanguagePicker {
                        Layout.fillWidth: true
                        languages: panel.vm ? panel.vm.languages : []
                        options: panel.vm ? panel.vm.commonLanguages : []
                        onLanguagesPicked: codes => panel.vm.languages = codes
                    }
                    FormCard.FormDelegateSeparator {}
                    FormCard.AbstractFormDelegate {
                        background: null
                        contentItem: RowLayout {
                            spacing: Theme.inlineSpacing
                            QQC2.Label {
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                                text: i18nc("@label subtitles filter row",
                                    "Hearing impaired")
                            }
                            Components.RadioSelector {
                                readonly property var modes: [
                                    "off", "include", "only"
                                ]
                                selectedIndex: Math.max(0,
                                    modes.indexOf(panel.vm ? panel.vm.hi : "off"))
                                actions: [
                                    Kirigami.Action {
                                        text: i18nc("@option:radio subtitle filter mode", "Off")
                                        onTriggered: if (panel.vm && panel.vm.hi !== "off") {
                                            panel.vm.hi = "off";
                                        }
                                    },
                                    Kirigami.Action {
                                        text: i18nc("@option:radio subtitle filter mode", "Include")
                                        onTriggered: if (panel.vm && panel.vm.hi !== "include") {
                                            panel.vm.hi = "include";
                                        }
                                    },
                                    Kirigami.Action {
                                        text: i18nc("@option:radio subtitle filter mode", "Only")
                                        onTriggered: if (panel.vm && panel.vm.hi !== "only") {
                                            panel.vm.hi = "only";
                                        }
                                    }
                                ]
                            }
                        }
                    }
                    FormCard.AbstractFormDelegate {
                        background: null
                        contentItem: RowLayout {
                            spacing: Theme.inlineSpacing
                            QQC2.Label {
                                Layout.fillWidth: true
                                elide: Text.ElideRight
                                text: i18nc("@label subtitles filter row",
                                    "Foreign parts only")
                            }
                            Components.RadioSelector {
                                readonly property var modes: [
                                    "off", "include", "only"
                                ]
                                selectedIndex: Math.max(0,
                                    modes.indexOf(panel.vm ? panel.vm.fpo : "off"))
                                actions: [
                                    Kirigami.Action {
                                        text: i18nc("@option:radio subtitle filter mode", "Off")
                                        onTriggered: if (panel.vm && panel.vm.fpo !== "off") {
                                            panel.vm.fpo = "off";
                                        }
                                    },
                                    Kirigami.Action {
                                        text: i18nc("@option:radio subtitle filter mode", "Include")
                                        onTriggered: if (panel.vm && panel.vm.fpo !== "include") {
                                            panel.vm.fpo = "include";
                                        }
                                    },
                                    Kirigami.Action {
                                        text: i18nc("@option:radio subtitle filter mode", "Only")
                                        onTriggered: if (panel.vm && panel.vm.fpo !== "only") {
                                            panel.vm.fpo = "only";
                                        }
                                    }
                                ]
                            }
                        }
                    }
                    FormCard.FormDelegateSeparator {}
                    FormCard.FormTextFieldDelegate {
                        label: i18nc("@label:textbox subtitles filter row",
                            "Release name (optional)")
                        text: panel.vm ? panel.vm.release : ""
                        placeholderText: i18nc(
                            "@info:placeholder subtitles release filter",
                            "e.g. 1080p, BluRay, REMUX")
                        onTextChanged: if (panel.vm) panel.vm.release = text
                        onAccepted: if (panel.vm) panel.vm.runSearch()
                    }
                    FormCard.FormButtonDelegate {
                        text: i18nc("@action:button", "Search")
                        icon.name: "edit-find"
                        enabled: panel.vm && panel.vm.state !== "notconfigured"
                        onClicked: panel.vm.runSearch()
                    }
                }
            }
        }

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
                    visible: panel.showCancelButton
                    text: i18nc("@action:button subtitles cancel", "Cancel")
                    onClicked: panel.vm.closeRequested()
                }
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
