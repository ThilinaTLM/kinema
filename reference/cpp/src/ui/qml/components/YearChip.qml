// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick

// Label-only pill rendered on top of poster cards to surface the
// release year. Mirrors `RatingChip` so the two chips look like
// siblings on opposite corners of the artwork.
PosterBadge {
    id: chip

    property int year: 0

    visible: year > 0
    text: year > 0 ? String(year) : ""
}
