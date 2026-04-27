// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Dense overview pane for full-width detail pages. Unlike BackdropHero, this
// is designed to live in the left side of an internal split view.
ColumnLayout {
    id: overview

    property string posterUrl
    property string backdropUrl
    property string title
    property int year: 0
    property var genres: []
    property var cast: []
    property int runtimeMinutes: 0
    property real rating: -1
    property string description
    property bool isUpcoming: false
    property string releaseDateText

    property Kirigami.Action primaryAction
    property Kirigami.Action secondaryAction
    property Kirigami.Action tertiaryAction

    spacing: Theme.groupSpacing

    Item {
        Layout.fillWidth: true
        Layout.preferredHeight: Kirigami.Units.gridUnit * 14

        Image {
            id: backdrop
            anchors.fill: parent
            source: overview.backdropUrl
                ? "image://kinema/backdrop?u=" + encodeURIComponent(overview.backdropUrl)
                : ""
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            cache: true
            sourceSize.width: 1280
            opacity: status === Image.Ready ? 1.0 : 0.0
            Behavior on opacity { NumberAnimation { duration: Kirigami.Units.longDuration } }
        }
        Rectangle {
            anchors.fill: parent
            color: Kirigami.Theme.alternateBackgroundColor
            visible: backdrop.status !== Image.Ready
        }
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                orientation: Gradient.Vertical
                GradientStop { position: 0.0; color: Qt.alpha(Theme.background, 0.20) }
                GradientStop { position: 0.65; color: Qt.alpha(Theme.background, 0.65) }
                GradientStop { position: 1.0; color: Theme.background }
            }
        }

        Item {
            id: posterFrame
            anchors.left: parent.left
            anchors.bottom: parent.bottom
            anchors.leftMargin: Theme.pageMargin
            anchors.bottomMargin: Theme.pageMargin
            width: Theme.posterMin
            height: Math.round(width * 1.5)

            Rectangle {
                anchors.fill: parent
                radius: Kirigami.Units.cornerRadius
                color: Kirigami.Theme.alternateBackgroundColor
                border.color: Qt.alpha(Theme.foreground, 0.16)
                border.width: 1
            }
            Image {
                id: poster
                anchors.fill: parent
                anchors.margins: 1
                source: overview.posterUrl
                    ? "image://kinema/poster?u=" + encodeURIComponent(overview.posterUrl)
                    : ""
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                cache: true
                sourceSize.width: Theme.posterMin
                visible: status === Image.Ready
            }
            Kirigami.Icon {
                anchors.centerIn: parent
                width: Kirigami.Units.iconSizes.large
                height: width
                source: "applications-multimedia"
                color: Qt.alpha(Theme.foreground, 0.35)
                visible: poster.status !== Image.Ready
            }
        }
    }

    ColumnLayout {
        Layout.fillWidth: true
        Layout.leftMargin: Theme.pageMargin
        Layout.rightMargin: Theme.pageMargin
        spacing: Theme.groupSpacing

        MetaSection {
            Layout.fillWidth: true
            title: overview.title
            year: overview.year
            genres: overview.genres
            runtimeMinutes: overview.runtimeMinutes
            rating: overview.rating
            description: overview.description
            isUpcoming: overview.isUpcoming
            releaseDateText: overview.releaseDateText
        }

        RowLayout {
            Layout.fillWidth: true
            visible: overview.primaryAction || overview.secondaryAction
                || overview.tertiaryAction
            spacing: Theme.inlineSpacing

            QQC2.Button {
                visible: overview.primaryAction !== null
                    && overview.primaryAction !== undefined
                icon.name: overview.primaryAction ? overview.primaryAction.icon.name : ""
                text: overview.primaryAction ? overview.primaryAction.text : ""
                enabled: overview.primaryAction ? overview.primaryAction.enabled : false
                highlighted: true
                onClicked: if (overview.primaryAction) overview.primaryAction.trigger()
            }
            QQC2.Button {
                visible: overview.secondaryAction !== null
                    && overview.secondaryAction !== undefined
                icon.name: overview.secondaryAction ? overview.secondaryAction.icon.name : ""
                text: overview.secondaryAction ? overview.secondaryAction.text : ""
                enabled: overview.secondaryAction ? overview.secondaryAction.enabled : false
                onClicked: if (overview.secondaryAction) overview.secondaryAction.trigger()
            }
            QQC2.Button {
                visible: overview.tertiaryAction !== null
                    && overview.tertiaryAction !== undefined
                icon.name: overview.tertiaryAction ? overview.tertiaryAction.icon.name : ""
                text: overview.tertiaryAction ? overview.tertiaryAction.text : ""
                enabled: overview.tertiaryAction ? overview.tertiaryAction.enabled : false
                onClicked: if (overview.tertiaryAction) overview.tertiaryAction.trigger()
            }
            Item { Layout.fillWidth: true }
        }

        Kirigami.Separator {
            Layout.fillWidth: true
            visible: castLabel.visible
        }

        ColumnLayout {
            Layout.fillWidth: true
            visible: castLabel.visible
            spacing: Theme.inlineSpacing

            Kirigami.Heading {
                level: 4
                text: i18nc("@title:section detail cast", "Cast")
            }
            QQC2.Label {
                id: castLabel
                Layout.fillWidth: true
                text: overview.cast.length > 0
                    ? overview.cast.slice(0, 8).join(i18nc("@text:list separator", ", "))
                    : ""
                visible: text.length > 0
                wrapMode: Text.Wrap
                maximumLineCount: 4
                elide: Text.ElideRight
                color: Theme.foreground
            }
        }
    }
}
