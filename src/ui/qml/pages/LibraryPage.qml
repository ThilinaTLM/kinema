// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

Kirigami.ScrollablePage {
    id: page

    objectName: "library"
    title: i18nc("@title:window", "Library")

    leftPadding: 0
    rightPadding: 0
    topPadding: Theme.pageMargin
    bottomPadding: Theme.pageBottomSpacing

    actions: [
        Kirigami.Action {
            icon.source: AppIcons.url("refresh-cw")
            icon.color: AppIcons.foreground
            text: i18nc("@action:button", "Refresh")
            displayHint: Kirigami.DisplayHint.IconOnly
            shortcut: StandardKey.Refresh
            onTriggered: libraryVm.refresh()
        }
    ]

    function sectionLabel(base, count) {
        return count > 0 ? i18nc("@label library tab with count", "%1 (%2)", base, count) : base;
    }

    ColumnLayout {
        id: stack
        width: page.width
        spacing: Theme.groupSpacing

        QQC2.TabBar {
            id: tabs
            Layout.fillWidth: true
            Layout.leftMargin: Theme.pageMargin
            Layout.rightMargin: Theme.pageMargin
            currentIndex: libraryVm.section
            onCurrentIndexChanged: if (libraryVm.section !== currentIndex) {
                libraryVm.section = currentIndex;
            }

            QQC2.TabButton {
                text: page.sectionLabel(i18nc("@label library tab", "Continue"),
                    libraryVm.continueCount)
            }
            QQC2.TabButton {
                text: page.sectionLabel(i18nc("@label library tab", "To Watch"),
                    libraryVm.toWatchCount)
            }
            QQC2.TabButton {
                text: page.sectionLabel(i18nc("@label library tab", "Watched"),
                    libraryVm.watchedCount)
            }
            QQC2.TabButton {
                text: page.sectionLabel(i18nc("@label library tab", "Upcoming"),
                    libraryVm.upcomingCount)
            }
        }

        Kirigami.PlaceholderMessage {
            Layout.fillWidth: true
            Layout.leftMargin: Theme.pageWideMargin
            Layout.rightMargin: Theme.pageWideMargin
            visible: libraryVm.empty
            icon.source: AppIcons.url("library")
            icon.color: AppIcons.foreground
            text: {
                switch (libraryVm.section) {
                case LibraryViewModel.Continue:
                    return i18nc("@info placeholder", "Nothing to continue.");
                case LibraryViewModel.Watched:
                    return i18nc("@info placeholder", "Nothing watched yet.");
                case LibraryViewModel.Upcoming:
                    return i18nc("@info placeholder", "No upcoming saved titles.");
                default:
                    return i18nc("@info placeholder", "Your Library is empty.");
                }
            }
            explanation: i18nc("@info placeholder", "Open a movie or series detail page and add it to your Library.")
        }

        GridView {
            id: grid
            Layout.fillWidth: true
            Layout.preferredHeight: contentHeight + Theme.pageMargin * 2
            visible: !libraryVm.empty
            interactive: false
            clip: false
            model: libraryVm.model
            leftMargin: Theme.pageMargin
            rightMargin: Theme.pageMargin
            topMargin: Theme.inlineSpacing
            bottomMargin: Theme.pageBottomSpacing

            readonly property int targetCellWidth: Theme.posterMin
                + Kirigami.Units.largeSpacing
            readonly property int availableWidth: Math.max(0,
                width - leftMargin - rightMargin)
            readonly property int columns: Math.max(1,
                Math.floor(availableWidth / targetCellWidth))
            readonly property int cellInset: Kirigami.Units.smallSpacing

            cellWidth: Math.floor(availableWidth / columns)
            cellHeight: Math.round((cellWidth - cellInset * 2) * 1.5)
                + Kirigami.Theme.defaultFont.pixelSize * 4

            delegate: Item {
                width: grid.cellWidth
                height: grid.cellHeight

                LibraryCard {
                    anchors.fill: parent
                    anchors.margins: grid.cellInset
                    posterUrl: model.posterUrl !== undefined ? model.posterUrl : ""
                    title: model.title !== undefined ? model.title : ""
                    subtitle: model.subtitle !== undefined ? model.subtitle : ""
                    progress: model.progress !== undefined ? model.progress : -1
                    watched: model.watched !== undefined ? model.watched : false
                    upcoming: model.upcoming !== undefined ? model.upcoming : false
                    releaseDateText: model.releaseDateText !== undefined
                        ? model.releaseDateText : ""
                    onClicked: libraryVm.activate(index)
                }
            }
        }
    }
}
