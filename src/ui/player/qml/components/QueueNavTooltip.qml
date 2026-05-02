// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Layouts
import dev.tlmtech.kinema.player

Rectangle {
    id: root

    property bool shown: false
    property string title: ""
    property string subtitle: ""
    property var chips: []
    property int maxWidth: Theme.gridUnit * 20

    visible: opacity > 0.001
    opacity: shown ? 1.0 : 0.0
    z: 10

    implicitWidth: Math.min(maxWidth,
        Math.max(titleText.implicitWidth,
                 subtitleText.visible ? subtitleText.implicitWidth : 0,
                 chipsRow.visible ? chipsRow.implicitWidth : 0)
        + Theme.spacing * 2)
    implicitHeight: content.implicitHeight + Theme.spacing * 2

    width: implicitWidth
    height: implicitHeight
    radius: Theme.radiusLg
    color: Theme.surfaceElev
    border.color: Theme.border
    border.width: 1

    Behavior on opacity { NumberAnimation { duration: Theme.fadeMs } }

    ColumnLayout {
        id: content
        anchors.fill: parent
        anchors.margins: Theme.spacing
        spacing: Theme.spacingXs

        Text {
            id: titleText
            Layout.fillWidth: true
            text: root.title
            color: Theme.foreground
            font.family: Theme.defaultFont.family
            font.pointSize: Theme.defaultFont.pointSize
            font.weight: Font.DemiBold
            elide: Text.ElideRight
            maximumLineCount: 1
            wrapMode: Text.NoWrap
        }

        Text {
            id: subtitleText
            Layout.fillWidth: true
            visible: text.length > 0
            text: root.subtitle
            color: Theme.foregroundDim
            font.family: Theme.smallFont.family
            font.pointSize: Theme.smallFont.pointSize
            elide: Text.ElideRight
            maximumLineCount: 1
            wrapMode: Text.NoWrap
        }

        MediaChips {
            id: chipsRow
            Layout.fillWidth: true
            visible: chips.length > 0
            chips: root.chips
        }
    }
}
