// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Compact strip of removable active-filter tokens. Sits below the
// `BrowseFilterBar` and only paints when at least one filter is
// active; collapses to zero height otherwise so the grid takes the
// full available area in the common case.
//
// Each chip is a native `QQC2.ToolButton` with a trailing × icon —
// styling tracks the user's Plasma theme automatically. No custom
// rectangles, no hand-rolled palette.
Item {
    id: root

    /// Bound to `browseVm.activeChips`.
    property var chips: []
    signal chipRemoved(int index)
    signal clearAllRequested()

    visible: chips.length > 0
    implicitHeight: visible
        ? row.implicitHeight + Theme.inlineSpacing
        : 0

    Behavior on implicitHeight {
        NumberAnimation { duration: Kirigami.Units.shortDuration }
    }

    RowLayout {
        id: row
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        anchors.leftMargin: Theme.pageMargin
        anchors.rightMargin: Theme.pageMargin
        spacing: Theme.inlineSpacing

        QQC2.Label {
            text: i18nc("@label prefix for active filter chip list",
                "Filters:")
            color: Kirigami.Theme.disabledTextColor
        }

        QQC2.ScrollView {
            Layout.fillWidth: true
            Layout.preferredHeight: chipFlow.implicitHeight
            contentWidth: chipFlow.implicitWidth
            contentHeight: chipFlow.implicitHeight
            QQC2.ScrollBar.horizontal.policy: QQC2.ScrollBar.AsNeeded
            QQC2.ScrollBar.vertical.policy: QQC2.ScrollBar.AlwaysOff
            clip: true

            Row {
                id: chipFlow
                spacing: Theme.inlineSpacing

                Repeater {
                    model: root.chips

                    delegate: QQC2.ToolButton {
                        id: chip
                        required property int index
                        required property var modelData

                        readonly property string chipLabel:
                            modelData.label !== undefined
                                ? modelData.label : ""

                        Accessible.name: i18nc("@info:whatsthis",
                            "Remove filter %1", chipLabel)
                        onClicked: root.chipRemoved(index)

                        // Custom contentItem so the × icon sits
                        // *after* the label without resorting to
                        // LayoutMirroring, which would also affect
                        // text reading direction.
                        contentItem: RowLayout {
                            spacing: Theme.inlineSpacing
                            QQC2.Label {
                                text: chip.chipLabel
                                color: Kirigami.Theme.textColor
                                elide: Text.ElideRight
                                Layout.fillWidth: true
                            }
                            Kirigami.Icon {
                                source: "edit-delete-remove-symbolic"
                                Layout.preferredWidth:
                                    Kirigami.Units.iconSizes.small
                                Layout.preferredHeight:
                                    Kirigami.Units.iconSizes.small
                                color: Kirigami.Theme.disabledTextColor
                            }
                        }
                    }
                }
            }
        }

        QQC2.ToolButton {
            text: i18nc("@action:button clear every active filter",
                "Clear all")
            icon.name: "edit-clear-history"
            display: QQC2.AbstractButton.TextBesideIcon
            onClicked: root.clearAllRequested()
        }
    }
}
