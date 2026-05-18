// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// One TMDB / Cinemeta tile: 2:3 poster, two-line wrapped title,
// year chip top-left, rating chip top-right, plus the shared
// poster context menu (Find Streams / Add to Library / Mark
// Watched / Copy Title / Open on TMDb \u2022 IMDb).
//
// Thin wrapper over `BasePosterCard` \u2014 the chassis owns the
// artwork frame, hover/focus, keyboard activation, right-click
// + menu key, and the title block; this file contributes the
// overlay chips, a few typed props (`tmdbId`, `imdbId`,
// `kindIndex`), and the context menu itself.
//
// Idempotent "open external" and "copy" items invoke the shell
// helpers directly so they need no view-model wiring. The three
// state-changing items (`findStreamsRequested`,
// `addToLibraryRequested`, `markWatchedRequested`) bubble up as
// row-agnostic signals; consumer delegates (`ContentRail`,
// `PosterGrid`, `SimilarCarousel`) attach the row's index when
// forwarding to the matching view-model slot.
//
// Consumers: `PosterGrid` (Search / Browse), `ContentRail`
// (Discover), `SimilarCarousel` (detail pages).
BasePosterCard {
    id: card

    // ---- Inputs --------------------------------------------------
    property int year: 0
    property real rating: -1

    /// TMDB id of the title behind this poster, if known. Drives
    /// the "Open on TMDb" menu item; 0 hides it.
    property int tmdbId: 0
    /// IMDb id of the title behind this poster, if known. Drives
    /// the "Open on IMDb" menu item; empty hides it. Search /
    /// Library posters carry IMDb; Discover / Browse don't.
    property string imdbId: ""
    /// `domain::MediaKind` value (0 = Movie, 1 = Series). Needed by
    /// `shell.openTmdbTitle` to pick the right URL segment and by
    /// the "Mark Watched" item which is movie-only.
    property int kindIndex: 0

    // ---- Menu-action signals -------------------------------------
    // Emitted from the context menu's three state-changing items.
    // PosterCard has no notion of its row index inside the parent
    // model, so the consumer's delegate captures `index` and
    // forwards each signal to the appropriate view-model slot.
    signal findStreamsRequested()
    signal addToLibraryRequested()
    signal markWatchedRequested()

    // `posterUrl`, `title`, `clicked()`, `rightClicked()` come
    // from the base.

    YearChip {
        year: card.year
        anchors {
            top: parent.top
            left: parent.left
            topMargin: Kirigami.Units.smallSpacing
            leftMargin: Kirigami.Units.smallSpacing
        }
    }

    RatingChip {
        rating: card.rating
        anchors {
            top: parent.top
            right: parent.right
            topMargin: Kirigami.Units.smallSpacing
            rightMargin: Kirigami.Units.smallSpacing
        }
    }

    // Right-click + menu key (Shift+F10) both reach the menu via
    // `BasePosterCard.rightClicked()`. We re-popup at the card's
    // current pointer / focus position; QQC2.Menu's default
    // placement anchors to the active focus item, which is the
    // card itself, so this works for both activation paths.
    onRightClicked: contextMenu.popup()

    KinemaMenu {
        id: contextMenu

        KinemaMenuItem {
            iconName: "info"
            label: i18nc("@action:inmenu poster card", "Details")
            onTriggered: card.clicked()
        }
        KinemaMenuItem {
            iconName: "list-video"
            label: i18nc("@action:inmenu poster card", "Streams")
            onTriggered: card.findStreamsRequested()
        }
        QQC2.MenuSeparator { }
        KinemaMenuItem {
            iconName: "library"
            label: i18nc("@action:inmenu poster card", "Add to Library")
            onTriggered: card.addToLibraryRequested()
        }
        KinemaMenuItem {
            iconName: "circle-check"
            label: i18nc("@action:inmenu poster card", "Mark Watched")
            // Series rows fall through to a passive notification
            // instead of a real mutation \u2014 see
            // `title_actions::markWatchedByTmdb`. We still surface
            // the item so the menu shape is uniform across kinds.
            onTriggered: card.markWatchedRequested()
        }
        QQC2.MenuSeparator { }
        KinemaMenuItem {
            iconName: "copy"
            label: i18nc("@action:inmenu poster card", "Copy Title")
            enabled: card.title.length > 0
            onTriggered: shell.copyToClipboard(card.title,
                i18nc("@info:status", "Title copied to clipboard"))
        }
        KinemaMenuItem {
            iconName: "external-link"
            label: i18nc("@action:inmenu poster card", "Open on TMDb")
            visible: card.tmdbId > 0
            onTriggered: shell.openTmdbTitle(card.tmdbId, card.kindIndex)
        }
        KinemaMenuItem {
            iconName: "external-link"
            label: i18nc("@action:inmenu poster card", "Open on IMDb")
            visible: card.tmdbId <= 0 && card.imdbId.length > 0
            onTriggered: shell.openImdbTitle(card.imdbId)
        }
    }
}
