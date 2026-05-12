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
// Filter chrome (release-name SearchField, Languages multi-select
// menu, HI / FPO sub-menus) lives on the parent `SubtitlesPage`
// header; this component is responsible only for the status line,
// the state-switched results region (idle / loading / error / empty
// / not-configured / ready), and the footer toolbar (Open file… /
// primary action) — all hosted via `ListSurface`.
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

        ListSurface {
            id: surface
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: Kirigami.Units.gridUnit * 12

            // Map the VM's string state into the QML enum. The
            // panel collapses idle / unknown to ListSurface.Idle so
            // the placeholder copy is unchanged from the old build.
            state: {
                if (!panel.vm) {
                    return ListSurface.Idle;
                }
                switch (panel.vm.state) {
                case "loading":       return ListSurface.Loading;
                case "error":         return ListSurface.Error;
                case "empty":         return ListSurface.Empty;
                case "notconfigured": return ListSurface.Custom;
                case "ready":         return ListSurface.Ready;
                }
                return ListSurface.Idle;
            }

            model: panel.vm ? panel.vm.results : null
            currentIndex: panel.vm ? panel.vm.selectedRow : -1
            listLeftMargin: 0
            listRightMargin: 0
            listSpacing: 0

            delegate: SubtitleListCard {
                required property int index
                required property var model
                rowIndex: index
                selected: index === (panel.vm
                    ? panel.vm.selectedRow : -1)
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
                vm: panel.vm
                onClicked: panel.vm.selectedRow = index
            }

            idleConfig: ({
                icon: "search",
                iconColor: AppIcons.foreground,
                title: i18nc("@info subtitles idle state",
                    "Pick a language and search.")
            })

            emptyConfig: ({
                icon: "search",
                iconColor: AppIcons.foreground,
                title: i18nc("@info subtitles empty state",
                    "No subtitles found."),
                explanation: i18nc(
                    "@info subtitles empty state explanation",
                    "Try widening the languages or removing the "
                    + "release filter.")
            })

            errorConfig: ({
                icon: "circle-alert",
                iconColor: AppIcons.negative,
                title: i18nc("@info subtitles error state",
                    "Search failed."),
                explanation: panel.vm ? panel.vm.errorText : "",
                actionText: i18nc("@action retry subtitles search",
                    "Retry"),
                actionIcon: "refresh-cw",
                onActionTriggered: () => {
                    if (panel.vm) panel.vm.runSearch();
                }
            })

            // `notconfigured` falls through to Custom because it
            // carries a settings action and copy that's not shaped
            // like a generic error.
            customComponent: Component {
                Kirigami.PlaceholderMessage {
                    width: Math.min(
                        parent.width - Theme.pageWideMargin * 2,
                        Theme.placeholderMaxWidth)
                    icon.source: AppIcons.url("settings")
                    icon.color: AppIcons.foreground
                    text: i18nc(
                        "@info subtitles not configured state",
                        "OpenSubtitles isn't configured yet.")
                    explanation: i18nc(
                        "@info subtitles not configured state explanation",
                        "Add credentials in Settings to search.")
                    helpfulAction: Kirigami.Action {
                        text: i18nc(
                            "@action open settings from subtitles page",
                            "Open settings\u2026")
                        icon.source: AppIcons.url("settings")
                        icon.color: AppIcons.foreground
                        onTriggered: panel.vm.openSettings()
                    }
                }
            }
        }

        QQC2.ToolBar {
            Layout.fillWidth: true
            contentItem: RowLayout {
                spacing: Theme.inlineSpacing

                QQC2.Button {
                    text: i18nc(
                        "@action:button subtitles open local file",
                        "Open file\u2026")
                    icon.source: AppIcons.url("folder-open")
                    icon.color: AppIcons.controlColor(enabled, false)
                    enabled: !!panel.vm
                    onClicked: localFileDialog.open()
                }
                Item { Layout.fillWidth: true }
                QQC2.Button {
                    text: panel.vm ? panel.vm.primaryActionText : ""
                    icon.source: AppIcons.url(
                        panel.vm ? panel.vm.primaryActionIcon : "",
                        AppIcons.controlColor(enabled, highlighted))
                    icon.color: AppIcons.controlColor(enabled, highlighted)
                    enabled: panel.vm && panel.vm.primaryActionEnabled
                    highlighted: true
                    onClicked: panel.vm.runPrimaryAction()
                }
            }
        }
    }
}
