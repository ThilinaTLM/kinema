// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// PosterCard variant for the Continue Watching rail: adds a thin
// progress bar overlay at the bottom of the poster and a "release
// line" subtitle beneath the title (e.g. "1080p — Movie.Name.2024").
//
// Forwards `clicked` (resume) and `removeRequested` /
// `chooseAnotherRequested` from the right-click context menu so the
// view-model can route each to `HistoryController`.
Item {
    id: card

    property string posterUrl
    property string title
    property string lastRelease    // e.g. "1080p — Some.Release.Name"
    property real progress: 0.0    // [0, 1]; <= 0 hides the bar

    signal clicked()
    signal removeRequested()
    signal chooseAnotherRequested()

    implicitWidth: Theme.posterMin
    implicitHeight: poster.height + meta.implicitHeight
        + Kirigami.Units.smallSpacing * 2

    transform: Scale {
        origin.x: card.width / 2
        origin.y: card.height / 2
        xScale: hoverArea.containsMouse ? 1.02 : 1.0
        yScale: hoverArea.containsMouse ? 1.02 : 1.0

        Behavior on xScale { NumberAnimation { duration: Kirigami.Units.shortDuration } }
        Behavior on yScale { NumberAnimation { duration: Kirigami.Units.shortDuration } }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: Kirigami.Units.smallSpacing

        Item {
            id: poster
            Layout.fillWidth: true
            Layout.preferredHeight: Math.round(width * 1.5)

            Rectangle {
                anchors.fill: parent
                radius: Kirigami.Units.cornerRadius
                color: Kirigami.Theme.alternateBackgroundColor
                border.color: Qt.alpha(Theme.foreground, 0.10)
                border.width: 1
            }

            Image {
                id: posterImage
                anchors.fill: parent
                anchors.margins: 1
                source: card.posterUrl
                    ? "image://kinema/poster?u=" + encodeURIComponent(card.posterUrl)
                    : ""
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                cache: true
                sourceSize.width: Theme.posterMax
                sourceSize.height: Math.round(Theme.posterMax * 1.5)
                visible: status === Image.Ready
            }

            Kirigami.Icon {
                visible: posterImage.status !== Image.Ready
                anchors.centerIn: parent
                width: Kirigami.Units.iconSizes.huge
                height: width
                source: "applications-multimedia"
                color: Qt.alpha(Theme.foreground, 0.35)
            }

            // Progress bar overlay along the poster's bottom edge.
            // Mirrors `ContinueWatchingCardDelegate`'s 4px thick bar.
            Rectangle {
                visible: card.progress > 0
                anchors {
                    left: parent.left
                    right: parent.right
                    bottom: parent.bottom
                    leftMargin: 1
                    rightMargin: 1
                    bottomMargin: 1
                }
                height: 4
                radius: 2
                color: Qt.alpha(Theme.background, 0.62)

                Rectangle {
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    width: parent.width * Math.max(0, Math.min(1, card.progress))
                    radius: 2
                    color: Theme.accent
                }
            }
        }

        ColumnLayout {
            id: meta
            spacing: 0
            Layout.fillWidth: true

            QQC2.Label {
                Layout.fillWidth: true
                text: card.title
                elide: Text.ElideRight
                maximumLineCount: 1
                font.pointSize: Theme.captionFont.pointSize + 1
                font.weight: Font.DemiBold
                color: Theme.foreground
            }

            QQC2.Label {
                Layout.fillWidth: true
                visible: card.lastRelease.length > 0
                text: card.lastRelease
                elide: Text.ElideRight
                font.pointSize: Theme.captionFont.pointSize
                color: Theme.disabled
            }
        }
    }

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
            icon.name: "media-playback-start"
            onTriggered: card.clicked()
        }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Choose another release…")
            icon.name: "view-list-details"
            onTriggered: card.chooseAnotherRequested()
        }
        QQC2.MenuSeparator {}
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Remove from history")
            icon.name: "edit-delete"
            onTriggered: card.removeRequested()
        }
    }

    Rectangle {
        anchors.fill: parent
        anchors.margins: -2
        radius: Kirigami.Units.cornerRadius + 2
        color: "transparent"
        border.color: Theme.accent
        border.width: 2
        visible: card.activeFocus
    }
}
