// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Shared "pill" overlay used on top of poster cards (rating, year,
// and any future micro-metadata). Renders as a semi-transparent
// rounded chip with optional leading icon and bold caption-sized
// text. Tint is baked into the icon via `AppIcons.url(id, color)`
// because `AppIconResolver` produces a pre-colored SVG, not a Qt
// mask — `Kirigami.Icon.color` has no effect on those.
Rectangle {
    id: badge

    // ---- Inputs --------------------------------------------------
    property string text: ""
    property string iconSource: ""
    property color  iconColor: Theme.accent

    radius: Kirigami.Units.cornerRadius
    color: Qt.alpha(Theme.background, 0.78)
    border.color: Qt.alpha(Theme.foreground, 0.18)
    border.width: 1

    implicitWidth: row.implicitWidth + Kirigami.Units.smallSpacing * 2
    implicitHeight: row.implicitHeight + Kirigami.Units.smallSpacing

    RowLayout {
        id: row
        anchors.centerIn: parent
        spacing: 2

        Kirigami.Icon {
            visible: badge.iconSource.length > 0
            source: badge.iconSource.length > 0
                ? AppIcons.url(badge.iconSource, badge.iconColor)
                : ""
            Layout.preferredWidth: Math.round(
                Kirigami.Units.iconSizes.small * 0.75)
            Layout.preferredHeight: Math.round(
                Kirigami.Units.iconSizes.small * 0.75)
        }

        QQC2.Label {
            text: badge.text
            font.pointSize: Theme.captionFont.pointSize
            font.bold: true
            color: Theme.foreground
        }
    }
}
