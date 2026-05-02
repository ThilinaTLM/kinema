// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// PosterCard variant for the Continue Watching rail. Same chrome
// as `PosterCard` (via `KinemaArtworkFrame`) plus:
//
//   * a thin progress bar overlay along the poster's bottom edge,
//   * an episode badge chip ("S01E02") on the poster for series,
//   * a right-click context menu that forwards resume / details /
//     streams / remove actions to the view-model.
Item {
    id: card

    // ---- Inputs --------------------------------------------------
    property string posterUrl
    property string title
    property string episodeSubtitle   // e.g. "S01E02"; empty for movies
    property real progress: 0.0    // [0, 1]; <= 0 hides the bar

    signal clicked()
    signal detailsRequested()
    signal streamsRequested()
    signal removeRequested()

    readonly property bool _hovered: hoverArea.containsMouse

    Kirigami.Theme.colorSet: Kirigami.Theme.View
    Kirigami.Theme.inherit: false

    implicitWidth: Theme.posterMin
    implicitHeight: poster.height + meta.implicitHeight
        + Kirigami.Units.smallSpacing * 2

    activeFocusOnTab: true
    Keys.onPressed: function (event) {
        if (event.key === Qt.Key_Return
            || event.key === Qt.Key_Enter
            || event.key === Qt.Key_Space) {
            card.clicked();
            event.accepted = true;
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: Kirigami.Units.smallSpacing

        KinemaArtworkFrame {
            id: poster
            Layout.fillWidth: true
            Layout.preferredHeight: Math.round(width * aspect)

            url: card.posterUrl
            aspect: 1.5
            fallbackIcon: "film"
            hovered: card._hovered
            focusRing: card._hovered || card.activeFocus
            progress: card.progress

            // Episode badge (top-left). Mirrors RatingChip styling.
            // Hidden for movies / entries without season+episode.
            EpisodeChip {
                episodeSubtitle: card.episodeSubtitle
                anchors {
                    top: parent.top
                    left: parent.left
                    topMargin: Kirigami.Units.smallSpacing
                    leftMargin: Kirigami.Units.smallSpacing
                }
            }
        }

        ColumnLayout {
            id: meta
            spacing: 0
            Layout.fillWidth: true

            Kirigami.Heading {
                Layout.fillWidth: true
                level: 5
                text: card.title
                elide: Text.ElideRight
                maximumLineCount: 1
                color: Kirigami.Theme.textColor
            }
        }
    }

    // Click + hover + right-click handling. We keep `MouseArea`
    // here (instead of `TapHandler` like `PosterCard`) so the
    // right-button context menu works.
    MouseArea {
        id: hoverArea
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        cursorShape: Qt.PointingHandCursor
        onClicked: function (mouse) {
            if (mouse.button === Qt.LeftButton) {
                card.clicked();
            } else if (mouse.button === Qt.RightButton) {
                contextMenu.popup();
            }
        }
    }

    QQC2.Menu {
        id: contextMenu
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Resume")
            icon.source: AppIcons.url("play")
            icon.color: AppIcons.controlColor(enabled, false)
            onTriggered: card.clicked()
        }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Details")
            icon.source: AppIcons.url("info")
            icon.color: AppIcons.controlColor(enabled, false)
            onTriggered: card.detailsRequested()
        }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Streams")
            icon.source: AppIcons.url("list-video")
            icon.color: AppIcons.controlColor(enabled, false)
            onTriggered: card.streamsRequested()
        }
        QQC2.MenuSeparator {}
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Remove")
            icon.source: AppIcons.url("trash-2")
            icon.color: AppIcons.controlColor(enabled, false)
            onTriggered: card.removeRequested()
        }
    }
}
