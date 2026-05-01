// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Smart rail used at the top of the Library page (Up Next /
// Airing Soon / Coming Up). Header on top, horizontal episode-card
// list below.
//
// Public surface:
//   * `model`         \u2014 a `LibraryRailModel*` from the C++ side.
//   * `title`         \u2014 i18n'd rail heading.
//   * `artworkShape`  \u2014 "thumbnail" (16:9 episode still) or
//                       "poster" (2:3 movie poster). Up Next /
//                       Airing Soon use the former; Coming Up
//                       uses the latter.
//   * `signal itemActivated(int row)`
//
// Self-hides via `visible: !rail.model || rail.model.empty` on
// the caller side; the page binds it directly.
ColumnLayout {
    id: rail

    property var model
    property string title: ""
    property string artworkShape: "thumbnail"

    signal itemActivated(int row)

    spacing: Theme.inlineSpacing

    readonly property real artworkAspect:
        artworkShape === "poster" ? 1.5 : 9 / 16
    readonly property string fallbackIcon:
        artworkShape === "poster" ? "film" : "tv"
    readonly property int cardWidth:
        artworkShape === "poster"
            ? Theme.posterMin
            : Kirigami.Units.gridUnit * 18

    // ---- Header ---------------------------------------------------
    RowLayout {
        Layout.fillWidth: true
        Layout.leftMargin: Theme.pageMargin
        Layout.rightMargin: Theme.pageMargin

        Kirigami.Heading {
            level: 3
            text: rail.title
            Layout.fillWidth: true
            elide: Text.ElideRight
        }
        QQC2.Label {
            visible: rail.model && rail.model.count > 0
            text: rail.model ? String(rail.model.count) : ""
            color: Theme.disabled
            font.pointSize: Theme.captionFont.pointSize
        }
    }

    // ---- List -----------------------------------------------------
    ListView {
        id: list
        Layout.fillWidth: true
        Layout.preferredHeight: cardPrototype.implicitHeight

        orientation: ListView.Horizontal
        clip: true
        spacing: Theme.groupSpacing
        leftMargin: Theme.pageMargin
        rightMargin: Theme.pageMargin
        boundsBehavior: Flickable.StopAtBounds
        cacheBuffer: width
        model: rail.model

        delegate: EpisodeRailCard {
            width: rail.cardWidth
            artworkAspect: rail.artworkAspect
            fallbackIcon: rail.fallbackIcon
            posterUrl: model.posterUrl !== undefined ? model.posterUrl : ""
            thumbnailUrl: model.thumbnailUrl !== undefined
                ? model.thumbnailUrl : ""
            primaryLine: model.primaryLine !== undefined
                ? model.primaryLine : ""
            secondaryLine: model.secondaryLine !== undefined
                ? model.secondaryLine : ""
            progress: model.progress !== undefined ? model.progress : -1

            onClicked: rail.itemActivated(index)
        }
    }

    // Sized prototype used to anchor the ListView's preferred height
    // to the card's natural height, mirroring `ContinueWatchingRail`.
    // `Layout.preferredWidth` (not `width`) so the QtQuick.Layouts
    // attached property treats the card as a managed sibling.
    EpisodeRailCard {
        id: cardPrototype
        visible: false
        Layout.preferredWidth: rail.cardWidth
        artworkAspect: rail.artworkAspect
    }
}
