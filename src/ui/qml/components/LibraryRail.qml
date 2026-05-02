// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Smart rail used on the Library page's Smart view (Up Next /
// Airing Soon / Recently Added). Header on top, horizontal
// `EpisodeRailCard` row below.
//
// Public surface:
//   * `model`         — a `LibraryRailModel*` from the C++ side.
//   * `title`         — i18n'd rail heading.
//   * `artworkShape`  — "thumbnail" (16:9 episode still) or
//                       "poster" (2:3 poster). Up Next + Airing
//                       Soon use thumbnails; Recently Added uses
//                       posters.
//   * `signal itemActivated(int row)`
//
// Self-hides via `visible: !rail.model || rail.model.empty` on the
// caller side. All Library rails share the same outer left/right
// margin (`Theme.pageMargin`) so headers and card strips line up
// vertically across rails.
ColumnLayout {
    id: rail

    property var model
    property string title: ""
    property string artworkShape: "thumbnail"

    signal itemActivated(int row)

    // Per-rail vertical structure. Header to ListView spacing is
    // `inlineSpacing`; outer rail-to-rail spacing is owned by the
    // parent ColumnLayout (Theme.sectionSpacing).
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
    // Mirrors `ContentRail`'s header on Discover: heading only, no
    // trailing count. The smart rails are capped at small sizes by
    // the view-model, so a count in the right gutter would just
    // duplicate visible state and break alignment with Discover.
    RowLayout {
        Layout.fillWidth: true
        Layout.leftMargin: Theme.pageMargin
        Layout.rightMargin: Theme.pageMargin
        spacing: Theme.inlineSpacing

        Kirigami.Heading {
            level: 3
            text: rail.title
            Layout.fillWidth: true
            elide: Text.ElideRight
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
            backdropUrl: model.backdropUrl !== undefined
                ? model.backdropUrl : ""
            thumbnailUrl: model.thumbnailUrl !== undefined
                ? model.thumbnailUrl : ""
            primaryLine: model.primaryLine !== undefined
                ? model.primaryLine : ""
            secondaryLine: model.secondaryLine !== undefined
                ? model.secondaryLine : ""
            tertiaryLine: model.tertiaryLine !== undefined
                ? model.tertiaryLine : ""
            progress: model.progress !== undefined ? model.progress : -1

            onClicked: rail.itemActivated(index)
        }
    }

    // Sized prototype used to anchor the ListView's preferred height
    // to the card's natural height, mirroring `ContinueWatchingRail`.
    //
    // Must use plain `width` rather than `Layout.preferredWidth`:
    // `visible: false` excludes the prototype from the parent
    // ColumnLayout, so attached `Layout.*` properties never get
    // applied. Without an explicit width the prototype falls back to
    // its own implicitWidth (gridUnit*16), and the rail row reserves
    // far more vertical space than the real 128 px-wide poster
    // delegates need.
    EpisodeRailCard {
        id: cardPrototype
        visible: false
        width: rail.cardWidth
        artworkAspect: rail.artworkAspect
        fallbackIcon: rail.fallbackIcon
        // Fill all three lines so the prototype reports the maximum
        // possible card height (movies with two lines coexist with
        // episodes with three lines in the same row, and the row
        // height needs to fit the tallest card).
        primaryLine: "\u00a0"
        secondaryLine: "\u00a0"
        tertiaryLine: "\u00a0"
    }
}
