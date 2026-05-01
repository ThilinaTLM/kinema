// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Small "S01E02" pill overlaid on poster cards. Hidden when the
// subtitle string is empty (movies or entries without season/episode).
// Mirrors `RatingChip` styling so the two chips feel cohesive.
Rectangle {
    id: chip

    property string episodeSubtitle   // e.g. "S01E02"; empty = hidden

    visible: episodeSubtitle.length > 0
    radius: Kirigami.Units.cornerRadius
    color: Qt.alpha(Theme.background, 0.78)
    border.color: Qt.alpha(Theme.foreground, 0.18)
    border.width: 1

    implicitWidth: label.implicitWidth + Kirigami.Units.smallSpacing * 2
    implicitHeight: label.implicitHeight + Kirigami.Units.smallSpacing

    QQC2.Label {
        id: label
        anchors.centerIn: parent
        text: chip.episodeSubtitle
        font.pointSize: Theme.captionFont.pointSize
        font.bold: true
        color: Theme.foreground
    }
}