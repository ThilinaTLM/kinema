// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Single-row sticky filter bar for Browse. Compact by design:
// every control is a native Kirigami / Plasma component so the bar
// matches the user's Plasma style and the page reserves no more
// vertical chrome than a plain `QQC2.ToolBar`.
//
// Layout (left to right):
//   * Movies / TV Series — `MediaKindSwitch` (Plasma `RadioSelector`).
//   * Genres ▾           — `GenresMenuButton` popup with the current
//                          selection count surfaced in its label.
//   * <fill>             — pushes the right-side controls.
//   * Sort ▾             — native `QQC2.ComboBox`.
//   * More filters       — opens `BrowseFiltersSheet` for date
//                          window / min rating / hide-obscure.
QQC2.ToolBar {
    id: bar

    /// `BrowseFiltersSheet` instance owned by the page.
    property var moreFiltersSheet

    Kirigami.Theme.colorSet: Kirigami.Theme.Header
    Kirigami.Theme.inherit: false

    // Padding matches the standard Kirigami toolbar so the bar
    // visually aligns with the global header above it.
    padding: Kirigami.Units.smallSpacing
    leftPadding: Kirigami.Units.largeSpacing
    rightPadding: Kirigami.Units.largeSpacing

    contentItem: RowLayout {
        spacing: Kirigami.Units.largeSpacing

        MediaKindSwitch {
            kind: browseVm.kind
            onActivated: function (newKind) {
                if (browseVm.kind !== newKind) {
                    browseVm.kind = newKind;
                }
            }
        }

        GenresMenuButton {
            sourceGenres: browseVm.availableGenres
            selectedIds: browseVm.genreIds
            onSelectionChanged: function (nextIds) {
                browseVm.genreIds = nextIds;
            }
        }

        Item { Layout.fillWidth: true }

        QQC2.ComboBox {
            id: sortCombo
            // Indices match `api::DiscoverSort` (Popularity=0,
            // ReleaseDate=1, Rating=2, TitleAsc=3) so we bind the
            // index directly to `browseVm.sort`.
            model: [
                i18nc("@item sort", "Most popular"),
                i18nc("@item sort", "Newest first"),
                i18nc("@item sort", "Highest rated"),
                i18nc("@item sort", "Title (A\u2013Z)")
            ]
            Accessible.name: i18nc("@label", "Sort by")
            currentIndex: browseVm.sort
            onActivated: if (currentIndex !== browseVm.sort) {
                browseVm.sort = currentIndex;
            }
        }

        QQC2.ToolButton {
            text: i18nc("@action:button open browse filters sheet",
                "More filters")
            icon.name: "configure"
            display: QQC2.AbstractButton.TextBesideIcon
            onClicked: if (bar.moreFiltersSheet) {
                bar.moreFiltersSheet.open();
            }
        }
    }
}
