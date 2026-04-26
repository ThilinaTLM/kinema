// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.components as Components

// Native Plasma segmented control for the Movies / TV Series toggle.
//
// Backed by `org.kde.kirigamiaddons.components.RadioSelector` —
// the official KDE segmented selector — so the look matches every
// other Plasma app the user has installed (Discover, Tokodon,
// NeoChat, etc.) instead of a hand-rolled pill.
//
// Public API kept stable so both `SearchPage` (header) and
// `BrowseFilterBar` consume it the same way:
//
//   property int kind             // 0 = Movie, 1 = Series
//   signal activated(int newKind)
Components.RadioSelector {
    id: root

    property int kind: 0
    signal activated(int newKind)

    consistentWidth: false

    // Use a separate Binding object so user clicks (which write to
    // `selectedIndex` directly) don't permanently break the
    // incoming `kind → selectedIndex` link. Without this, an
    // external state change from `applyPreset` would fail to
    // update the segmented control once the user has interacted
    // with it.
    Binding {
        target: root
        property: "selectedIndex"
        value: root.kind
        restoreMode: Binding.RestoreNone
    }

    onSelectedIndexChanged: if (selectedIndex !== kind) {
        root.activated(selectedIndex);
    }

    actions: [
        Kirigami.Action {
            text: i18nc("@option:radio media kind", "Movies")
            icon.name: "video-x-generic"
        },
        Kirigami.Action {
            text: i18nc("@option:radio media kind", "TV Series")
            icon.name: "view-media-recent"
        }
    ]
}
