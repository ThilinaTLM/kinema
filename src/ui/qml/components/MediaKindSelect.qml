// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.components as Components

import dev.tlmtech.kinema.app

// Movies / TV Series toggle, shared by `BrowsePage` and `SearchPage`.
//
// Thin wrapper around `Components.SegmentedButton` that bakes in the
// two actions (Movies, TV Series), keeps the i18n strings in one
// place, and exposes a small VM-friendly API:
//
//   property int kind              // 0 = Movie, 1 = Series
//   signal activated(int newKind)  // user-driven changes only
//
// Idiomatic call site:
//
//   MediaKindSelect {
//       kind: vm.kind
//       onActivated: vm.kind = newKind
//   }
//
// Visuals come straight from upstream `Components.SegmentedButton`
// (Breeze button palette, fused outer corners, 1 px shared seam,
// `alternateBackgroundColor` on checked, `focusColor` on
// hover/focus). Internal padding is `mediumSpacing`, matching
// `QQC2.ToolButton` / `Kirigami.ActionToolBar` chrome — which is
// the convention this control sits next to in both pages. We do
// not override padding to match `Kirigami.SearchField` height: in
// Breeze, segmented filter chrome is intentionally slightly more
// compact than text-input controls.
Components.SegmentedButton {
    id: root

    property int kind: 0
    signal activated(int newKind)

    actions: [
        Kirigami.Action {
            text: i18nc("@option:radio media kind", "Movies")
            icon.source: AppIcons.url("film")
            icon.color: checked ? AppIcons.accent : AppIcons.foreground
            checkable: true
            checked: root.kind === 0
            onTriggered: if (root.kind !== 0) {
                root.activated(0);
            }
        },
        Kirigami.Action {
            text: i18nc("@option:radio media kind", "TV Series")
            icon.source: AppIcons.url("tv")
            icon.color: checked ? AppIcons.accent : AppIcons.foreground
            checkable: true
            checked: root.kind === 1
            onTriggered: if (root.kind !== 1) {
                root.activated(1);
            }
        }
    ]
}
