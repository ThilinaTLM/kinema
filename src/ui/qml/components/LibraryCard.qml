// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Library list-view tile. Sibling of `PosterCard` —
// `KinemaArtworkFrame` provides the same shadow lift, focus ring,
// hover tint, and rounded-corner clipping; the card adds:
//
//   * a `RatingChip` overlay (top-right) when the entry has an
//     IMDb rating,
//   * a `StatusChip` overlay (top-left) for Watched / Upcoming
//     entries,
//   * a thin progress bar along the poster's bottom edge for
//     in-progress titles (driven by the frame's built-in overlay),
//   * a right-click context menu for Open / toggle Watched / Remove
//     from Library.
//
// No more hover-revealed kebab button — the menu is accessible via
// right-click everywhere card-style UIs in the app use it.
Item {
    id: card

    // ---- Inputs --------------------------------------------------
    property string posterUrl
    property string title
    property string subtitle
    property real rating: -1
    property real progress: -1
    property bool watched: false
    property bool upcoming: false

    signal clicked()
    signal removeRequested()
    signal toggleWatchedRequested()

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

            // Watched / Upcoming badge (top-left). Self-hides when
            // both flags are false.
            StatusChip {
                watched: card.watched
                upcoming: card.upcoming
                anchors {
                    top: parent.top
                    left: parent.left
                    topMargin: Kirigami.Units.smallSpacing
                    leftMargin: Kirigami.Units.smallSpacing
                }
            }

            // IMDb rating chip (top-right). Self-hides when
            // `rating <= 0`.
            RatingChip {
                rating: card.rating
                anchors {
                    top: parent.top
                    right: parent.right
                    topMargin: Kirigami.Units.smallSpacing
                    rightMargin: Kirigami.Units.smallSpacing
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

            QQC2.Label {
                Layout.fillWidth: true
                visible: card.subtitle.length > 0
                text: card.subtitle
                elide: Text.ElideRight
                font: Kirigami.Theme.smallFont
                color: Kirigami.Theme.disabledTextColor
            }
        }
    }

    // Click + hover + right-click handling, mirroring the pattern
    // used by the Continue Watching rail card.
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
            text: i18nc("@action:inmenu", "Open")
            icon.source: AppIcons.url("play")
            icon.color: AppIcons.controlColor(enabled, false)
            onTriggered: card.clicked()
        }
        QQC2.MenuItem {
            text: card.watched
                ? i18nc("@action:inmenu", "Mark as Unwatched")
                : i18nc("@action:inmenu", "Mark as Watched")
            icon.source: AppIcons.url(card.watched
                ? "circle-dashed" : "circle-check")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: !card.upcoming
            onTriggered: card.toggleWatchedRequested()
        }
        QQC2.MenuSeparator {}
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Remove from Library\u2026")
            icon.source: AppIcons.url("library")
            icon.color: AppIcons.negative
            onTriggered: card.removeRequested()
        }
    }
}
