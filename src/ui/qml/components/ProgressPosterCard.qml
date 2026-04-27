// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// PosterCard variant for the Continue Watching rail. Shares the
// same `Kirigami.ShadowedImage` chrome and hover-elevation pattern
// as `PosterCard` so the two card variants feel like siblings, and
// adds:
//
//   * a thin progress bar overlay along the poster's bottom edge,
//   * a release-line subtitle ("1080p — Movie.Name.2024"),
//   * a right-click context menu that forwards remove / pick-another
//     actions to the view-model.
Item {
    id: card

    // ---- Inputs --------------------------------------------------
    property string posterUrl
    property string title
    property string lastRelease    // e.g. "1080p — Some.Release.Name"
    property real progress: 0.0    // [0, 1]; <= 0 hides the bar

    signal clicked()
    signal removeRequested()
    signal chooseAnotherRequested()

    // Single source of truth for the hover-elevation state.
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

    // ---- Visual chrome ------------------------------------------
    ColumnLayout {
        anchors.fill: parent
        spacing: Kirigami.Units.smallSpacing

        // Same poster frame as `PosterCard`: distance-field shader
        // means the artwork is clipped to the rounded corners and
        // the shadow lifts cleanly on hover.
        Kirigami.ShadowedImage {
            id: poster
            Layout.fillWidth: true
            Layout.preferredHeight: Math.round(width * 1.5)

            radius: Kirigami.Units.cornerRadius
            color: Kirigami.Theme.alternateBackgroundColor

            source: card.posterUrl
                ? "image://kinema/poster?u=" + encodeURIComponent(card.posterUrl)
                : ""
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            readonly property int _srcW: Math.min(
                card.width * 2, Theme.posterMax * 2)
            sourceSize.width: _srcW
            sourceSize.height: Math.round(_srcW * 1.5)

            border.color: card._hovered || card.activeFocus
                ? Kirigami.Theme.focusColor
                : Qt.alpha(Kirigami.Theme.textColor, 0.12)
            border.width: card.activeFocus ? 2 : 1

            shadow.size: card._hovered
                ? Kirigami.Units.gridUnit
                : Kirigami.Units.smallSpacing
            shadow.yOffset: card._hovered
                ? Kirigami.Units.smallSpacing
                : 1
            shadow.color: Qt.alpha(Kirigami.Theme.textColor,
                card._hovered ? 0.40 : 0.18)

            Behavior on shadow.size {
                NumberAnimation { duration: Kirigami.Units.shortDuration }
            }
            Behavior on shadow.yOffset {
                NumberAnimation { duration: Kirigami.Units.shortDuration }
            }

            Kirigami.Icon {
                visible: poster.status !== Image.Ready
                anchors.centerIn: parent
                width: Kirigami.Units.iconSizes.huge
                height: width
                source: "applications-multimedia"
                color: Kirigami.Theme.disabledTextColor
            }

            // Hover tint, rounded-corner-clipped via the same shader.
            Kirigami.ShadowedRectangle {
                anchors.fill: parent
                radius: poster.radius
                color: Kirigami.Theme.hoverColor
                opacity: card._hovered ? 0.18 : 0
                Behavior on opacity {
                    NumberAnimation { duration: Kirigami.Units.shortDuration }
                }
            }

            // Progress bar overlay along the poster's bottom edge.
            // Mirrors the original 4px-thick bar; the track sits
            // inside the inset so it doesn't clip the rounded
            // corners.
            Item {
                visible: card.progress > 0
                anchors {
                    left: parent.left
                    right: parent.right
                    bottom: parent.bottom
                    leftMargin: Kirigami.Units.smallSpacing
                    rightMargin: Kirigami.Units.smallSpacing
                    bottomMargin: Kirigami.Units.smallSpacing
                }
                height: 4

                Rectangle {
                    anchors.fill: parent
                    radius: 2
                    color: Qt.alpha(Kirigami.Theme.backgroundColor, 0.62)
                }

                Rectangle {
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    width: parent.width
                        * Math.max(0, Math.min(1, card.progress))
                    radius: 2
                    color: Kirigami.Theme.highlightColor
                }
            }
        }

        // ---- Title + release line ----------------------------------
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
                visible: card.lastRelease.length > 0
                text: card.lastRelease
                elide: Text.ElideRight
                font: Kirigami.Theme.smallFont
                color: Kirigami.Theme.disabledTextColor
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
}
