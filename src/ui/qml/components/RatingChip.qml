// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Tiny "★ 8.4" pill rendered on top of poster cards. Hidden when
// the rating isn't known (the underlying TMDB row sometimes omits
// `voteAverage`). Sits on the chip background colorSet so it pops
// over light or dark posters consistently.
Rectangle {
    id: chip

    property real rating: -1

    visible: rating > 0
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
            // `rating` is a Breeze "favourite" star glyph; it tracks
            // the current colour scheme so it stays legible on both
            // light and dark surfaces.
            source: "rating"
            Layout.preferredWidth: Kirigami.Units.iconSizes.small
            Layout.preferredHeight: Kirigami.Units.iconSizes.small
            color: Theme.accent
        }

        QQC2.Label {
            text: chip.rating > 0 ? chip.rating.toFixed(1) : ""
            font.pointSize: Theme.captionFont.pointSize
            font.bold: true
            color: Theme.foreground
        }
    }
}
