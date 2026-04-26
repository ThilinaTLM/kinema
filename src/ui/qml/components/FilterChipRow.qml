// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Horizontal strip of removable filter chips. The Browse page binds
// `chips` to `browseVm.activeChips`, a `QVariantList` of
// `{ kind, label, payload? }` maps. Each chip's × button calls
// `browseVm.removeChip(index)` which the VM resolves back to the
// matching filter setter.
//
// The chip row scrolls horizontally when its width exceeds the
// available space, but stays single-row to keep the page header
// compact. Shown only when at least one chip exists.
QQC2.ScrollView {
    id: root

    property var chips: []
    signal chipRemoved(int index)

    visible: chips.length > 0
    contentWidth: chipFlow.implicitWidth
    contentHeight: chipFlow.implicitHeight
    QQC2.ScrollBar.horizontal.policy: QQC2.ScrollBar.AsNeeded
    QQC2.ScrollBar.vertical.policy: QQC2.ScrollBar.AlwaysOff

    Row {
        id: chipFlow
        spacing: Kirigami.Units.smallSpacing
        leftPadding: Kirigami.Units.largeSpacing
        rightPadding: Kirigami.Units.largeSpacing

        Repeater {
            model: root.chips
            delegate: Rectangle {
                required property int index
                required property var modelData

                radius: height / 2
                color: Kirigami.Theme.alternateBackgroundColor
                border.color: Qt.alpha(Theme.foreground, 0.18)
                border.width: 1
                implicitHeight: chipRow.implicitHeight
                    + Kirigami.Units.smallSpacing * 2
                implicitWidth: chipRow.implicitWidth
                    + Kirigami.Units.largeSpacing * 2

                RowLayout {
                    id: chipRow
                    anchors.centerIn: parent
                    spacing: Kirigami.Units.smallSpacing

                    QQC2.Label {
                        text: modelData.label !== undefined
                            ? modelData.label : ""
                        elide: Text.ElideRight
                        font: Theme.captionFont
                        color: Theme.foreground
                    }
                    QQC2.ToolButton {
                        icon.name: "edit-clear"
                        icon.width: Kirigami.Units.iconSizes.small
                        icon.height: Kirigami.Units.iconSizes.small
                        flat: true
                        // The first chip ("Movies"/"TV Series")
                        // doesn't really "remove" — it toggles the
                        // kind. We still let the VM handle it, but
                        // hide the × on chips that have no real
                        // remove semantics so the affordance stays
                        // honest. Per BrowseViewModel, the kind chip
                        // toggles to the other kind on remove —
                        // surfaced as a tooltip rather than a ×.
                        visible: modelData.kind !== "kind"
                        onClicked: root.chipRemoved(index)
                    }
                }
            }
        }
    }
}
