// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// "Genres ▾" toolbar button that opens a checkable popup menu.
//
// Replaces the previous `GenreChipsRow` — chips wrapped or scrolled
// poorly on narrow widths and ate a full row of vertical space.
// A popup keeps the filter bar to a single compact line; the
// selected count surfaces inline so users still see at a glance how
// many genres are filtering the grid.
//
// Bound to:
//   * `sourceGenres`  — `browseVm.availableGenres` ({id,name,checked})
//   * `selectedIds`   — `browseVm.genreIds`
//   * `selectionChanged(nextIds)` — the page wires this back to
//     `browseVm.genreIds = nextIds`.
QQC2.Button {
    id: root

    /// Bound to `browseVm.availableGenres`.
    property var sourceGenres: []
    /// Bound to `browseVm.genreIds`. Read-only here; toggling a
    /// menu entry emits `selectionChanged` with the new list.
    property var selectedIds: []
    signal selectionChanged(var nextIds)

    readonly property int selectedCount: (selectedIds || []).length
    readonly property bool loading: sourceGenres.length === 0

    icon.name: "view-categories"
    text: selectedCount > 0
        ? i18ncp("@action:button genres button with active count",
            "Genres (%1)", "Genres (%1)", selectedCount)
        : i18nc("@action:button", "Genres")
    display: QQC2.AbstractButton.TextBesideIcon
    checkable: true
    checked: menu.opened
    enabled: !loading

    onClicked: if (menu.opened) {
        menu.close();
    } else {
        menu.open();
    }

    QQC2.Menu {
        id: menu
        // Anchor the menu under the button. Width tracks the button
        // up to a sensible cap so long genre names ("Science Fiction
        // & Fantasy") don't truncate on the first popup.
        y: root.height
        implicitWidth: Math.max(root.width,
            Kirigami.Units.gridUnit * 14)

        QQC2.MenuItem {
            text: i18nc("@action:inmenu clear all genre selections",
                "Clear all genres")
            icon.name: "edit-clear-history"
            enabled: root.selectedCount > 0
            onTriggered: root.selectionChanged([])
        }
        QQC2.MenuSeparator { }

        Repeater {
            model: root.sourceGenres

            delegate: QQC2.MenuItem {
                required property var modelData
                text: modelData.name !== undefined ? modelData.name : ""
                checkable: true
                checked: modelData.checked === true
                onTriggered: {
                    const id = modelData.id;
                    let next = (root.selectedIds || []).slice();
                    const idx = next.indexOf(id);
                    if (checked && idx < 0) {
                        next.push(id);
                    } else if (!checked && idx >= 0) {
                        next.splice(idx, 1);
                    } else {
                        return;
                    }
                    root.selectionChanged(next);
                }
            }
        }
    }
}
