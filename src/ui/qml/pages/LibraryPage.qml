// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

Kirigami.Page {
    id: page

    objectName: "library"
    title: i18nc("@title:window", "Library")

    leftPadding: 0
    rightPadding: 0
    topPadding: 0
    bottomPadding: 0

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

    // ---- removal confirmation dialog ----------------------------
    Kirigami.Dialog {
        id: removeDialog
        title: i18nc("@title:dialog", "Remove from Library?")

        property int targetRow: -1
        property string targetTitle: ""

        standardButtons: Kirigami.Dialog.NoButton

        customFooterActions: [
            Kirigami.Action {
                text: i18nc("@action:button", "Cancel")
                icon.source: AppIcons.url("x")
                icon.color: AppIcons.foreground
                onTriggered: removeDialog.close()
            },
            Kirigami.Action {
                text: i18nc("@action:button", "Remove")
                icon.source: AppIcons.url("trash-2")
                icon.color: AppIcons.negative
                onTriggered: {
                    libraryVm.removeFromLibrary(removeDialog.targetRow, deleteTrackingCheck.checked);
                    removeDialog.close();
                }
            }
        ]

        ColumnLayout {
            spacing: Kirigami.Units.smallSpacing
            QQC2.Label {
                text: removeDialog.targetTitle.length > 0
                    ? i18nc("@info", "\u201c%1\u201d will be removed from your Library.", removeDialog.targetTitle)
                    : i18nc("@info", "This title will be removed from your Library.")
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
            QQC2.CheckBox {
                id: deleteTrackingCheck
                text: i18nc("@option:check", "Permanently delete watched state tracking data")
                checked: false
            }
        }
    }

    // ---- header: merged title + filter bar ----------------------
    header: PageHeaderBar {
        id: filterBar
        title: page.title
        pageActions: page.actions

        Item { Layout.fillWidth: true }

        FilterMenuButton {
            Layout.alignment: Qt.AlignVCenter
            axisLabel: i18nc("@action:button library view filter", "View")
            icon.source: AppIcons.url("library")
            active: libraryVm.section !== LibraryViewModel.ToWatch
            options: [
                { value: LibraryViewModel.Continue,
                  label: i18nc("@item library view", "Continue (%1)", libraryVm.continueCount) },
                { value: LibraryViewModel.ToWatch,
                  label: i18nc("@item library view", "To Watch (%1)", libraryVm.toWatchCount) },
                { value: LibraryViewModel.Watched,
                  label: i18nc("@item library view", "Watched (%1)", libraryVm.watchedCount) },
                { value: LibraryViewModel.Upcoming,
                  label: i18nc("@item library view", "Upcoming (%1)", libraryVm.upcomingCount) }
            ]
            currentValue: libraryVm.section
            onActivated: v => libraryVm.setSection(v)
        }
    }

    // ---- body ---------------------------------------------------
    GridView {
        id: grid
        anchors.fill: parent
        visible: !libraryVm.empty
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.VerticalFlick
        cacheBuffer: cellHeight * 2

        model: libraryVm.model

        leftMargin: Theme.pageMargin
        rightMargin: Theme.pageMargin
        topMargin: Theme.pageMargin
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
                onRemoveRequested: {
                    removeDialog.targetRow = index;
                    removeDialog.targetTitle = model.title !== undefined ? model.title : "";
                    removeDialog.open();
                }
                onToggleWatchedRequested: libraryVm.toggleWatched(index)
            }
        }
    }

    Kirigami.PlaceholderMessage {
        anchors.centerIn: parent
        width: Math.min(parent.width - Theme.pageWideMargin * 2,
            Theme.placeholderMaxWidth)
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
}
