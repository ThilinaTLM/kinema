// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

Kirigami.Page {
    id: page

    title: i18nc("@title:window", "Downloads")
    objectName: "downloads"

    leftPadding: 0
    rightPadding: 0
    topPadding: 0
    bottomPadding: 0

    // Filter pill model. Order matches DownloadsViewModel.Filter
    // (FilterAll, FilterActive, FilterCompleted, FilterFailed,
    // FilterPinned). Counts come from the VM so the labels stay
    // honest as rows transition.
    readonly property var filterDefs: [
        { key: 0,
          text: i18nc("@option:filter downloads", "All"),
          count: downloadsVm.totalCount },
        { key: 1,
          text: i18nc("@option:filter downloads", "Active"),
          count: downloadsVm.activeCount },
        { key: 2,
          text: i18nc("@option:filter downloads", "Completed"),
          count: downloadsVm.completedCount },
        { key: 3,
          text: i18nc("@option:filter downloads", "Failed"),
          count: downloadsVm.failedCount },
        { key: 4,
          text: i18nc("@option:filter downloads", "Pinned"),
          count: downloadsVm.pinnedCount },
    ]

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // ---- Aggregate header strip ----------------------------
        Kirigami.AbstractApplicationHeader {
            Layout.fillWidth: true
            Layout.preferredHeight: Kirigami.Units.gridUnit * 3.5

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: Kirigami.Units.largeSpacing
                anchors.rightMargin: Kirigami.Units.largeSpacing
                spacing: Kirigami.Units.largeSpacing

                Kirigami.Heading {
                    level: 3
                    text: i18nc("@title", "Downloads")
                }

                QQC2.Label {
                    visible: downloadsVm.totalDownloadRateText.length > 0
                    text: i18nc("@info aggregate downstream rate",
                        "↓ %1", downloadsVm.totalDownloadRateText)
                    color: Kirigami.Theme.textColor
                    font.weight: Font.DemiBold
                }

                Item { Layout.fillWidth: true }

                // Cache budget bar
                ColumnLayout {
                    spacing: 0
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 14
                    Layout.alignment: Qt.AlignVCenter
                    QQC2.Label {
                        Layout.fillWidth: true
                        text: i18nc("@info cache used / budget",
                            "Cache %1 of %2",
                            downloadsVm.cacheSizeText,
                            downloadsVm.cacheBudgetText)
                        color: Kirigami.Theme.disabledTextColor
                        font.pointSize: Kirigami.Theme.smallFont.pointSize
                        horizontalAlignment: Text.AlignRight
                    }
                    QQC2.ProgressBar {
                        Layout.fillWidth: true
                        from: 0
                        to: 1
                        value: downloadsVm.cacheUsageFraction
                    }
                }

                QQC2.ToolButton {
                    icon.name: "edit-clear-history"
                    text: i18nc("@action:button run cache eviction now",
                        "Run eviction")
                    display: QQC2.AbstractButton.TextBesideIcon
                    onClicked: downloadsVm.runEvictionNow()
                    QQC2.ToolTip.text: i18nc("@info:tooltip run eviction",
                        "Drop ephemeral cache entries until the budget is satisfied.")
                    QQC2.ToolTip.visible: hovered
                }

                QQC2.ToolButton {
                    icon.name: "overflow-menu"
                    text: i18nc("@action:button bulk actions", "Bulk")
                    display: QQC2.AbstractButton.IconOnly
                    QQC2.ToolTip.text: i18nc("@info:tooltip bulk actions",
                        "Bulk actions")
                    QQC2.ToolTip.visible: hovered
                    onClicked: bulkMenu.popup()

                    QQC2.Menu {
                        id: bulkMenu
                        QQC2.MenuItem {
                            text: i18nc("@action:inmenu", "Clear finished")
                            icon.name: "edit-clear-list"
                            enabled: downloadsVm.completedCount > 0
                            onTriggered: downloadsVm.clearFinished()
                        }
                        QQC2.MenuItem {
                            text: i18nc("@action:inmenu", "Clear failed")
                            icon.name: "edit-clear-list"
                            enabled: downloadsVm.failedCount > 0
                            onTriggered: downloadsVm.clearFailed()
                        }
                    }
                }
            }
        }

        // ---- Filter pills row ----------------------------------
        Rectangle {
            Layout.fillWidth: true
            implicitHeight: filterRow.implicitHeight
                + Kirigami.Units.smallSpacing * 2
            color: Kirigami.Theme.backgroundColor

            RowLayout {
                id: filterRow
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                anchors.leftMargin: Kirigami.Units.largeSpacing
                anchors.rightMargin: Kirigami.Units.largeSpacing
                spacing: Kirigami.Units.smallSpacing

                Repeater {
                    model: page.filterDefs
                    delegate: QQC2.Button {
                        required property var modelData
                        flat: !checked
                        checkable: true
                        checked: downloadsVm.filter === modelData.key
                        text: modelData.count > 0
                            ? i18nc("@label filter pill with count",
                                "%1 (%2)", modelData.text, modelData.count)
                            : modelData.text
                        onToggled: {
                            if (checked) {
                                downloadsVm.filter = modelData.key;
                            } else {
                                checked = true;
                            }
                        }
                    }
                }
                Item { Layout.fillWidth: true }
            }
        }

        Kirigami.Separator {
            Layout.fillWidth: true
            visible: downloadsVm.items.count > 0
        }

        // ---- Empty states --------------------------------------
        Kirigami.PlaceholderMessage {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: downloadsVm.items.count === 0
            icon.name: downloadsVm.filter === 0
                ? "download"
                : "view-list-text"
            text: switch (downloadsVm.filter) {
            case 1:
                return i18nc("@info:placeholder", "No active downloads")
            case 2:
                return i18nc("@info:placeholder", "Nothing finished yet")
            case 3:
                return i18nc("@info:placeholder", "No failed downloads")
            case 4:
                return i18nc("@info:placeholder", "Nothing pinned offline")
            default:
                return i18nc("@info:placeholder", "No downloads yet")
            }
            explanation: downloadsVm.filter === 0
                ? i18nc("@info:placeholder explanation",
                    "Click the \u2b07 Download button on a stream to start "
                    + "a full background download (it keeps going whether "
                    + "you watch or not). Click \u25b6 Play to stream on "
                    + "demand \u2014 only the bytes the player needs are "
                    + "fetched, and the session quiesces when you stop "
                    + "watching.")
                : ""

            QQC2.Button {
                visible: downloadsVm.filter === 0
                Layout.alignment: Qt.AlignHCenter
                text: i18nc("@action:button browse to find streams",
                    "Browse")
                icon.name: "search"
                onClicked: mainController.navigateToBrowseRequested()
            }
        }

        // ---- Downloads list ------------------------------------
        QQC2.ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: downloadsVm.items.count > 0
            clip: true

            ListView {
                id: list
                model: downloadsVm.items
                spacing: Kirigami.Units.smallSpacing
                topMargin: Kirigami.Units.smallSpacing
                bottomMargin: Kirigami.Units.smallSpacing
                leftMargin: Kirigami.Units.largeSpacing
                rightMargin: Kirigami.Units.largeSpacing

                delegate: DownloadRow {
                    width: list.width - list.leftMargin - list.rightMargin
                }
            }
        }
    }
}
