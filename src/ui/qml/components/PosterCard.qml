// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// One TMDB / Cinemeta tile: 2:3 poster, two-line wrapped title,
// year chip top-left, rating chip top-right.
//
// Thin wrapper over `BasePosterCard` — the chassis owns the
// artwork frame, hover/focus, keyboard activation, and the title
// block; this file only contributes the two overlay chips and a
// couple of typed props.
//
// Consumers: `PosterGrid` (Search / Browse), `ContentRail`
// (Discover), `SimilarCarousel` (detail pages).
BasePosterCard {
    id: card

    // ---- Inputs --------------------------------------------------
    property int year: 0
    property real rating: -1

    // `posterUrl`, `title`, `clicked()` come from the base. No
    // subtitle on Discover-style cards — the base hides the
    // subtitle label automatically when empty.

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
}
