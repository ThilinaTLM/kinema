// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

// Library page shell. Two views, switched in-place by a Smart switch
// sitting next to the page title:
//
//   * Smart \u2014 horizontal rails (Up Next / Airing Soon /
//     Recently Added). No filter chrome. Read-only browse surface
//     focused on "what to do tonight". Title reads "Smart Library".
//   * List  \u2014 Browse-style filterable grid: Kind / Status /
//     Genres / Sort dropdowns in the header, advanced dialog for
//     date window / min rating / hide watched. Title reads
//     "Library".
//
// The toggle state is persisted via `config::LibrarySettings`
// (KConfig group `[Library]`, key `smartMode`, default true) and
// surfaced through `LibraryViewModel::smartMode`. The page reads/
// writes that property exclusively \u2014 there is no in-memory
// mirror to keep in sync.
Kirigami.Page {
    id: page

    objectName: "library"
    // Title morphs with the Smart toggle so the switch needs no
    // separate label \u2014 the title itself communicates the mode.
    title: libraryVm.smartMode
        ? i18nc("@title:window", "Smart Library")
        : i18nc("@title:window", "Library")

    leftPadding: 0
    rightPadding: 0
    topPadding: 0
    bottomPadding: 0

    actions: [
        Kirigami.Action {
            id: clearFiltersAction
            icon.source: AppIcons.url("eraser")
            icon.color: AppIcons.foreground
            text: i18nc("@action:button reset library filters",
                "Clear filters")
            // Only relevant in List mode; hidden in Smart since
            // Smart has no filter axes.
            visible: !libraryVm.smartMode && libraryVm.filtersActive
            displayHint: Kirigami.DisplayHint.IconOnly
            onTriggered: libraryVm.resetFilters()
        },
        Kirigami.Action {
            icon.source: AppIcons.url("refresh-cw")
            icon.color: AppIcons.foreground
            text: i18nc("@action:button", "Refresh")
            displayHint: Kirigami.DisplayHint.IconOnly
            shortcut: StandardKey.Refresh
            onTriggered: libraryVm.refresh()
        }
    ]

    // ---- removal confirmation dialog ----------------------------
    Kirigami.Dialog {
        id: removeDialog
        title: i18nc("@title:dialog", "Remove from Library?")

        property int targetRow: -1
        property string targetTitle: ""

        function openFor(row, t) {
            targetRow = row;
            targetTitle = t;
            open();
        }

        standardButtons: Kirigami.Dialog.NoButton

        customFooterActions: [
            Kirigami.Action {
                text: i18nc("@action:button", "Cancel")
                icon.source: AppIcons.url("x")
                icon.color: AppIcons.foreground
                onTriggered: removeDialog.close()
            },
            Kirigami.Action {
                text: i18nc("@action:button", "Remove")
                icon.source: AppIcons.url("library")
                icon.color: AppIcons.negative
                onTriggered: {
                    libraryVm.removeFromLibrary(removeDialog.targetRow);
                    removeDialog.close();
                }
            }
        ]

        QQC2.Label {
            text: removeDialog.targetTitle.length > 0
                ? i18nc("@info",
                    "\u201c%1\u201d will be removed from your "
                    + "Library. Your watched and playback history "
                    + "for it is preserved.",
                    removeDialog.targetTitle)
                : i18nc("@info",
                    "This title will be removed from your Library. "
                    + "Your watched and playback history for it is "
                    + "preserved.")
            wrapMode: Text.WordWrap
        }
    }

    // ---- advanced filters dialog (List mode only) --------------
    Kirigami.Dialog {
        id: libraryAdvancedDialog
        title: i18nc("@title:dialog library advanced filters",
            "More filters")

        footer: QQC2.DialogButtonBox {
            QQC2.Button {
                text: i18nc("@action:button reset all library filters",
                    "Reset all filters")
                icon.source: AppIcons.url("eraser")
                icon.color: AppIcons.foreground
                visible: libraryVm.filtersActive
                QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.ActionRole
                onClicked: {
                    libraryVm.resetFilters();
                    libraryAdvancedDialog.close();
                }
            }
            QQC2.Button {
                text: i18nc("@action:button close dialog", "Close")
                QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.AcceptRole
                onClicked: libraryAdvancedDialog.close()
            }
        }
        preferredWidth: Kirigami.Units.gridUnit * 26

        FormCard.FormCard {
            FormCard.FormComboBoxDelegate {
                text: i18nc("@label library filter", "Released")
                model: [
                    i18nc("@item date window", "Past month"),
                    i18nc("@item date window", "Past 3 months"),
                    i18nc("@item date window", "This year"),
                    i18nc("@item date window", "Past 3 years"),
                    i18nc("@item date window", "Any time")
                ]
                currentIndex: libraryVm.dateWindow
                onActivated: idx => libraryVm.dateWindow = idx
            }
            FormCard.FormComboBoxDelegate {
                text: i18nc("@label library filter", "Minimum rating")
                readonly property var pctValues: [0, 50, 60, 70, 75, 80, 85, 90]
                model: [
                    i18nc("@item rating threshold", "Any"),
                    i18nc("@item rating threshold", "\u2605 5+"),
                    i18nc("@item rating threshold", "\u2605 6+"),
                    i18nc("@item rating threshold", "\u2605 7+"),
                    i18nc("@item rating threshold", "\u2605 7.5+"),
                    i18nc("@item rating threshold", "\u2605 8+"),
                    i18nc("@item rating threshold", "\u2605 8.5+"),
                    i18nc("@item rating threshold", "\u2605 9+")
                ]
                currentIndex: Math.max(0,
                    pctValues.indexOf(libraryVm.minRatingPct))
                onActivated: idx => {
                    const v = pctValues[idx];
                    if (libraryVm.minRatingPct !== v) {
                        libraryVm.minRatingPct = v;
                    }
                }
            }
            FormCard.FormSwitchDelegate {
                text: i18nc("@option:check library filter",
                    "Hide watched")
                description: i18nc("@info",
                    "Hide titles you've fully watched.")
                checked: libraryVm.hideWatched
                onToggled: libraryVm.hideWatched = checked
            }
        }
    }

    // ---- header: title + mode toggle + filter row ---------------
    header: PageHeaderBar {
        id: filterBar
        title: page.title
        pageActions: page.actions
        visible: !libraryVm.libraryEmpty
        advancedFiltersDialog: !libraryVm.smartMode
            ? libraryAdvancedDialog : null
        advancedFilterCount:
            (libraryVm.dateWindow !== LibraryViewModel.DateWindowAny ? 1 : 0)
            + (libraryVm.minRatingPct > 0 ? 1 : 0)
            + (libraryVm.hideWatched ? 1 : 0)

        // ---- Smart toggle, left-aligned -------------------------
        // Bare switch: the page title ("Smart Library" / "Library")
        // is the visible label. `onToggled` (not `onCheckedChanged`)
        // keeps the binding loop-free if `smartMode` is ever set
        // programmatically.
        QQC2.Switch {
            Layout.alignment: Qt.AlignVCenter
            checked: libraryVm.smartMode
            onToggled: libraryVm.smartMode = checked
            QQC2.ToolTip.text: i18nc("@info:tooltip",
                "Toggle smart library view")
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        }

        // Spacer pushes the filter widgets to the right.
        Item { Layout.fillWidth: true }

        // ---- List-mode filter widgets ---------------------------
        // Hidden in Smart mode so the bar stays uncluttered.
        FilterMenuButton {
            Layout.alignment: Qt.AlignVCenter
            visible: !libraryVm.smartMode
            axisLabel: i18nc("@action:button library filter", "Kind")
            icon.source: AppIcons.url("clapperboard")
            active: libraryVm.kind !== LibraryViewModel.KindFilter.All
            options: [
                { value: LibraryViewModel.KindFilter.All,
                  label: i18nc("@item library kind", "All") },
                { value: LibraryViewModel.KindFilter.Movies,
                  label: i18nc("@item library kind", "Movies") },
                { value: LibraryViewModel.KindFilter.Series,
                  label: i18nc("@item library kind", "TV Series") }
            ]
            currentValue: libraryVm.kind
            onActivated: v => libraryVm.kind = v
        }
        FilterMenuButton {
            Layout.alignment: Qt.AlignVCenter
            visible: !libraryVm.smartMode
            axisLabel: i18nc("@action:button library filter", "Status")
            icon.source: AppIcons.url("circle-check")
            active: libraryVm.status !== LibraryViewModel.StatusFilter.All
            options: [
                { value: LibraryViewModel.StatusFilter.All,
                  label: i18nc("@item library status", "All") },
                { value: LibraryViewModel.StatusFilter.Continue,
                  label: i18nc("@item library status", "Continue") },
                { value: LibraryViewModel.StatusFilter.ToWatch,
                  label: i18nc("@item library status", "To Watch") },
                { value: LibraryViewModel.StatusFilter.Watched,
                  label: i18nc("@item library status", "Watched") },
                { value: LibraryViewModel.StatusFilter.Upcoming,
                  label: i18nc("@item library status", "Upcoming") }
            ]
            currentValue: libraryVm.status
            onActivated: v => libraryVm.status = v
        }
        FilterMenuButton {
            Layout.alignment: Qt.AlignVCenter
            visible: !libraryVm.smartMode
            axisLabel: i18nc("@action:button library filter", "Genres")
            icon.source: AppIcons.url("tags")
            multiSelect: true
            active: libraryVm.genreIds.length > 0
            enabled: libraryVm.availableGenres.length > 0
            options: {
                const out = [];
                const src = libraryVm.availableGenres || [];
                for (let i = 0; i < src.length; ++i) {
                    out.push({
                        value: src[i].id,
                        label: src[i].name !== undefined
                            ? src[i].name : ""
                    });
                }
                return out;
            }
            currentValues: libraryVm.genreIds
            onMultiActivated: list => libraryVm.genreIds = list
        }
        FilterMenuButton {
            Layout.alignment: Qt.AlignVCenter
            visible: !libraryVm.smartMode
            axisLabel: i18nc("@action:button library sort", "Sort")
            icon.source: AppIcons.url("arrow-up-down")
            active: libraryVm.sort !== LibraryViewModel.SortMode.RecentlyAdded
            options: [
                { value: LibraryViewModel.SortMode.RecentlyAdded,
                  label: i18nc("@item library sort",
                    "Recently added") },
                { value: LibraryViewModel.SortMode.Title,
                  label: i18nc("@item library sort", "Title (A\u2013Z)") },
                { value: LibraryViewModel.SortMode.ReleaseDate,
                  label: i18nc("@item library sort", "Release date") },
                { value: LibraryViewModel.SortMode.Rating,
                  label: i18nc("@item library sort", "Rating") }
            ]
            currentValue: libraryVm.sort
            onActivated: v => libraryVm.sort = v
        }
    }

    // ---- empty-library placeholder (no titles saved) -----------
    Kirigami.PlaceholderMessage {
        anchors.centerIn: parent
        width: Math.min(parent.width - Theme.pageWideMargin * 2,
            Theme.placeholderMaxWidth)
        visible: libraryVm.libraryEmpty
        icon.source: AppIcons.url("library")
        icon.color: AppIcons.foreground
        text: i18nc("@info placeholder", "Your Library is empty.")
        explanation: i18nc("@info placeholder",
            "Open a movie or series detail page and add it to your "
            + "Library.")
    }

    // ---- body: StackLayout switching Smart \u2194 List ---------
    // StackLayout sizes its children to its own size, but only
    // when they declare `Layout.fillWidth` / `Layout.fillHeight`.
    // Without the explicit bindings, plain `Item` and `Flickable`
    // children fall back to their (zero) implicit size and nothing
    // renders.
    StackLayout {
        anchors.fill: parent
        visible: !libraryVm.libraryEmpty
        currentIndex: libraryVm.smartMode ? 0 : 1

        LibrarySmartView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            vm: libraryVm
        }

        LibraryListView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            vm: libraryVm
            onRemoveRequested: (row, title) =>
                removeDialog.openFor(row, title)
        }
    }
}
