// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Detail-page hero: full-bleed backdrop image with a gradient mask,
// a 2:3 poster overlay on the bottom-left, and a meta column on the
// right (title / chips / description / action button row).
//
// Sizes pull from `Theme.qml` so they scale with the user's KDE
// font / DPI. On narrow viewports the layout collapses to a single
// column (poster on top, meta below) via a Kirigami breakpoint.
Item {
    id: hero

    // ---- inputs ------------------------------------------------
    property string posterUrl
    property string backdropUrl
    property string title
    property int year: 0
    property var genres: []
    property int runtimeMinutes: 0
    property real rating: -1
    property string description
    property bool isUpcoming: false
    property string releaseDateText

    /// Optional Kirigami.Action instances surfaced as buttons inside
    /// the hero's meta column. Each action lands as a QQC2.Button
    /// so labels stay visible even when the icon is missing from
    /// the active theme.
    property Kirigami.Action primaryAction
    property Kirigami.Action secondaryAction
    property Kirigami.Action tertiaryAction

    readonly property bool narrow: width
        < Kirigami.Units.gridUnit * 40

    implicitHeight: narrow
        ? (Theme.heroHeight + posterImage.implicitHeight)
        : Theme.heroHeight

    // ---- backdrop image with gradient mask ---------------------
    Image {
        id: backdrop
        anchors.fill: parent
        source: hero.backdropUrl
            ? "image://kinema/backdrop?u=" + encodeURIComponent(hero.backdropUrl)
            : ""
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        cache: true
        sourceSize.width: 1920
        opacity: status === Image.Ready ? 1.0 : 0.0
        Behavior on opacity { NumberAnimation {
            duration: Kirigami.Units.longDuration } }
    }
    Rectangle {
        id: backdropFallback
        anchors.fill: parent
        visible: backdrop.status !== Image.Ready
        color: Kirigami.Theme.alternateBackgroundColor
    }

    // Gradient mask: dark at the bottom so the meta column reads
    // legibly over any backdrop colour. Kept linear and asymmetric
    // (top is mostly translucent) so the image still breathes.
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            orientation: Gradient.Vertical
            GradientStop { position: 0.0
                color: Qt.alpha(Theme.background, 0.55) }
            GradientStop { position: 0.55
                color: Qt.alpha(Theme.background, 0.85) }
            GradientStop { position: 1.0
                color: Theme.background }
        }
    }

    // ---- foreground content -----------------------------------
    GridLayout {
        anchors.fill: parent
        anchors.margins: Kirigami.Units.largeSpacing * 2
        columnSpacing: Kirigami.Units.largeSpacing * 2
        rowSpacing: Kirigami.Units.largeSpacing
        columns: hero.narrow ? 1 : 2

        // Poster.
        Item {
            Layout.alignment: hero.narrow
                ? Qt.AlignHCenter
                : (Qt.AlignLeft | Qt.AlignBottom)
            Layout.preferredWidth: Theme.posterMax
            Layout.preferredHeight: Math.round(Theme.posterMax * 1.5)

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
                source: hero.posterUrl
                    ? "image://kinema/poster?u=" + encodeURIComponent(hero.posterUrl)
                    : ""
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                cache: true
                sourceSize.width: Theme.posterMax
                visible: status === Image.Ready
            }
            Kirigami.Icon {
                anchors.centerIn: parent
                width: Kirigami.Units.iconSizes.huge
                height: width
                source: "applications-multimedia"
                color: Qt.alpha(Theme.foreground, 0.35)
                visible: posterImage.status !== Image.Ready
            }
        }

        // Meta column + actions.
        ColumnLayout {
            Layout.alignment: Qt.AlignBottom
            Layout.fillWidth: true
            spacing: Kirigami.Units.largeSpacing

            MetaSection {
                Layout.fillWidth: true
                title: hero.title
                year: hero.year
                genres: hero.genres
                runtimeMinutes: hero.runtimeMinutes
                rating: hero.rating
                description: hero.description
                isUpcoming: hero.isUpcoming
                releaseDateText: hero.releaseDateText
            }

            // Action button row. Buttons render only for non-null
            // Kirigami.Action assignments, so a host that omits the
            // secondary action gets a tighter strip without hand-
            // wiring visibility.
            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                visible: hero.primaryAction || hero.secondaryAction
                    || hero.tertiaryAction

                QQC2.Button {
                    visible: hero.primaryAction !== null
                        && hero.primaryAction !== undefined
                    icon.name: hero.primaryAction
                        ? hero.primaryAction.icon.name : ""
                    text: hero.primaryAction
                        ? hero.primaryAction.text : ""
                    enabled: hero.primaryAction
                        ? hero.primaryAction.enabled : false
                    highlighted: true
                    onClicked: {
                        if (hero.primaryAction) {
                            hero.primaryAction.trigger();
                        }
                    }
                }
                QQC2.Button {
                    visible: hero.secondaryAction !== null
                        && hero.secondaryAction !== undefined
                    icon.name: hero.secondaryAction
                        ? hero.secondaryAction.icon.name : ""
                    text: hero.secondaryAction
                        ? hero.secondaryAction.text : ""
                    enabled: hero.secondaryAction
                        ? hero.secondaryAction.enabled : false
                    onClicked: {
                        if (hero.secondaryAction) {
                            hero.secondaryAction.trigger();
                        }
                    }
                }
                QQC2.Button {
                    visible: hero.tertiaryAction !== null
                        && hero.tertiaryAction !== undefined
                    icon.name: hero.tertiaryAction
                        ? hero.tertiaryAction.icon.name : ""
                    text: hero.tertiaryAction
                        ? hero.tertiaryAction.text : ""
                    enabled: hero.tertiaryAction
                        ? hero.tertiaryAction.enabled : false
                    onClicked: {
                        if (hero.tertiaryAction) {
                            hero.tertiaryAction.trigger();
                        }
                    }
                }
                Item { Layout.fillWidth: true }
            }
        }
    }
}
