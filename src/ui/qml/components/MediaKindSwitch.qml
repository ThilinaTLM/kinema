// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// Connected-pill segmented control for the Movies / TV Series toggle.
//
// Replaces the earlier `Components.RadioSelector` wrapper because
// the latter renders the two options with a `RowLayout.spacing`
// gap that we cannot reach from outside the upstream component;
// the segments read as two adjacent buttons rather than one
// grouped switch. This implementation draws a single rounded
// outer border around both segments, removes inter-button
// spacing, and paints a thin separator between them so the unit
// reads as a true segmented control.
//
// Public API (kept stable for `SearchPage` and `BrowseFilterBar`):
//
//   property int kind             // 0 = Movie, 1 = Series
//   signal activated(int newKind)
//
// Theme-driven only — no hard-coded colours, no fixed pixel
// values. The pill height tracks `Kirigami.Units.gridUnit * 2` so
// it lines up with adjacent `QQC2.ToolButton`s in a toolbar.
Item {
    id: root

    property int kind: 0
    signal activated(int newKind)

    implicitHeight: Kirigami.Units.gridUnit * 2
    implicitWidth: row.implicitWidth

    // Outer rounded border — drawn once around the whole control
    // so the two segments read as a single unit.
    Rectangle {
        anchors.fill: parent
        radius: Kirigami.Units.cornerRadius
        color: "transparent"
        border.width: 1
        border.color: Qt.rgba(Kirigami.Theme.textColor.r,
            Kirigami.Theme.textColor.g,
            Kirigami.Theme.textColor.b, 0.25)
    }

    QQC2.ButtonGroup {
        id: group
        exclusive: true
    }

    RowLayout {
        id: row
        anchors.fill: parent
        spacing: 0

        Repeater {
            id: segments
            model: [
                { text: i18nc("@option:radio media kind", "Movies"),
                  iconName: "video-x-generic" },
                { text: i18nc("@option:radio media kind", "TV Series"),
                  iconName: "view-media-recent" }
            ]

            delegate: QQC2.AbstractButton {
                id: segment
                required property var modelData
                required property int index

                Layout.fillHeight: true
                Layout.minimumWidth: Kirigami.Units.gridUnit * 6

                checkable: true
                checked: root.kind === index
                QQC2.ButtonGroup.group: group

                onClicked: if (root.kind !== index) {
                    root.activated(index);
                }

                background: Rectangle {
                    // Match the outer border radius on the outer
                    // edges only; clip the inner edge by drawing a
                    // square cover rectangle over the corner that
                    // would otherwise round into the separator.
                    radius: Kirigami.Units.cornerRadius
                    color: segment.checked
                        ? Qt.rgba(Kirigami.Theme.highlightColor.r,
                            Kirigami.Theme.highlightColor.g,
                            Kirigami.Theme.highlightColor.b, 0.20)
                        : (segment.hovered
                            ? Qt.rgba(Kirigami.Theme.textColor.r,
                                Kirigami.Theme.textColor.g,
                                Kirigami.Theme.textColor.b, 0.06)
                            : "transparent")

                    // Square the inner edge of each segment so the
                    // pair forms one connected pill. Cover the
                    // rounded corners that face the separator with
                    // a same-coloured square strip.
                    Rectangle {
                        visible: segment.index === 0
                        anchors {
                            top: parent.top
                            bottom: parent.bottom
                            right: parent.right
                        }
                        width: parent.radius
                        color: parent.color
                    }
                    Rectangle {
                        visible: segment.index === segments.count - 1
                        anchors {
                            top: parent.top
                            bottom: parent.bottom
                            left: parent.left
                        }
                        width: parent.radius
                        color: parent.color
                    }
                }

                contentItem: RowLayout {
                    spacing: Kirigami.Units.smallSpacing

                    Item {
                        Layout.preferredWidth: Kirigami.Units.smallSpacing
                    }

                    Kirigami.Icon {
                        source: segment.modelData.iconName
                        implicitWidth: Kirigami.Units.iconSizes.sizeForLabels
                        implicitHeight: Kirigami.Units.iconSizes.sizeForLabels
                        color: segment.checked
                            ? Kirigami.Theme.highlightColor
                            : Kirigami.Theme.textColor

                        Behavior on color {
                            ColorAnimation {
                                duration: Kirigami.Units.shortDuration
                                easing.type: Easing.InOutCubic
                            }
                        }
                    }

                    QQC2.Label {
                        Layout.alignment: Qt.AlignVCenter
                        Layout.fillWidth: true
                        text: segment.modelData.text
                        color: Kirigami.Theme.textColor
                        font.bold: segment.checked
                        elide: Text.ElideRight
                    }

                    Item {
                        Layout.preferredWidth: Kirigami.Units.smallSpacing
                    }
                }
            }
        }
    }

    // Internal separator drawn between the two segments. Lives
    // outside the RowLayout so it doesn't consume cell width;
    // positioned via binding to the first segment's width.
    Rectangle {
        x: row.children.length > 0 && row.children[0]
            ? row.children[0].width
            : 0
        y: Kirigami.Units.smallSpacing
        width: 1
        height: parent.height - Kirigami.Units.smallSpacing * 2
        color: Qt.rgba(Kirigami.Theme.textColor.r,
            Kirigami.Theme.textColor.g,
            Kirigami.Theme.textColor.b, 0.20)
    }
}
