// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick

import dev.tlmtech.kinema.app

// Tiny "★ 8.4" pill rendered on top of poster cards. Hidden when
// the rating isn't known (the underlying TMDB row sometimes omits
// `voteAverage`). Thin wrapper over `PosterBadge` so rating/year/
// future overlays share identical chrome.
PosterBadge {
    id: chip

    property real rating: -1

    visible: rating > 0
    iconSource: "star-filled"
    iconColor: "white"
    text: rating > 0 ? rating.toFixed(1) : ""
}
