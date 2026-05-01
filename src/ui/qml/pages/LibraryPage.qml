// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

// Library: a Browse-style filterable grid of curated titles, with
// three smart rails (`Up Next`, `Airing soon`, `Coming up`) above
// the grid. The rails self-hide when filters are active so they
// don't fight the narrowed view below.
//
// Chrome:
//
//   * `header:` is a single merged `PageHeaderBar` that inlines the
//     page title with the basic filters (Kind, Status, Genres,
//     Sort) and a `More filters\u2026 (N)` button. The advanced
//     dialog (`libraryAdvancedDialog`) hosts the Released-window,
//     Min-rating, and Hide-watched switches.
//   * The body is a single `GridView` whose header (`gridHeader`)
//     hosts the smart rails + the results sub-heading. Putting the
//     rails inside the grid header rather than alongside avoids a
//     nested-Flickable wheel/flick conflict.
//   * Empty placeholders cover the full page when (a) the library
//     itself is empty, or (b) filters yield zero matches.
Kirigami.Page {
    id: page

    objectName: "library"
    title: i18nc("@title:window", "Library")

    leftPadding: 0
    rightPadding: 0
    topPadding: 0
    bottomPadding: 0

    actions: [
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

    // ---- advanced filters dialog --------------------------------
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

    // ---- header: merged title + filter bar ----------------------
    header: PageHeaderBar {
        id: filterBar
        title: page.title
        pageActions: page.actions
        visible: !libraryVm.libraryEmpty
        advancedFiltersDialog: libraryAdvancedDialog
        // Defaults: dateWindow = Any time (4), minRatingPct = 0,
        // hideWatched = false. The "(N)" badge counts deviations.
        advancedFilterCount:
            (libraryVm.dateWindow !== LibraryViewModel.DateWindowAny ? 1 : 0)
            + (libraryVm.minRatingPct > 0 ? 1 : 0)
            + (libraryVm.hideWatched ? 1 : 0)

        Item { Layout.fillWidth: true }

        // ---- Kind (All / Movies / Series) -----------------------
        FilterMenuButton {
            Layout.alignment: Qt.AlignVCenter
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

        // ---- Status ---------------------------------------------
        FilterMenuButton {
            Layout.alignment: Qt.AlignVCenter
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

        // ---- Genres (multi-select) ------------------------------
        FilterMenuButton {
            Layout.alignment: Qt.AlignVCenter
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

        // ---- Sort -----------------------------------------------
        FilterMenuButton {
            Layout.alignment: Qt.AlignVCenter
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

    // ---- "no matches" placeholder (filters narrow to 0) --------
    Kirigami.PlaceholderMessage {
        anchors.centerIn: parent
        width: Math.min(parent.width - Theme.pageWideMargin * 2,
            Theme.placeholderMaxWidth)
        visible: !libraryVm.libraryEmpty && libraryVm.empty
        icon.source: AppIcons.url("search")
        icon.color: AppIcons.foreground
        text: i18nc("@info placeholder", "No matches")
        explanation: i18nc("@info placeholder",
            "Nothing in your Library matches these filters. Try "
            + "clearing some.")
        helpfulAction: Kirigami.Action {
            icon.source: AppIcons.url("eraser")
            icon.color: AppIcons.foreground
            text: i18nc("@action:button", "Reset filters")
            onTriggered: libraryVm.resetFilters()
        }
    }

    // ---- body: grid + rail-bearing header ---------------------
    GridView {
        id: grid
        anchors.fill: parent
        visible: !libraryVm.libraryEmpty && !libraryVm.empty
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.VerticalFlick
        cacheBuffer: cellHeight * 2

        model: libraryVm.model

        leftMargin: Theme.pageMargin
        rightMargin: Theme.pageMargin
        topMargin: 0
        bottomMargin: Theme.pageBottomSpacing

        readonly property int targetCellWidth:
            Theme.posterMin + Kirigami.Units.largeSpacing
        readonly property int availableWidth: Math.max(0,
            width - leftMargin - rightMargin)
        readonly property int columns: Math.max(1,
            Math.floor(availableWidth / targetCellWidth))
        readonly property int cellInset: Kirigami.Units.smallSpacing

        cellWidth: Math.floor(availableWidth / columns)
        cellHeight: Math.round((cellWidth - cellInset * 2) * 1.5)
            + Kirigami.Theme.defaultFont.pixelSize * 4

        // Spans the full grid width; the rails inherit `grid.width`
        // and use their own page margins for the inner content.
        header: ColumnLayout {
            width: grid.width
            spacing: Theme.sectionSpacing

            Item { Layout.preferredHeight: Theme.pageTopSpacing }

            // Smart rails. Hidden as a group when any filter is
            // active so they don't compete with a narrowed grid.
            ColumnLayout {
                Layout.fillWidth: true
                spacing: Theme.sectionSpacing
                visible: !libraryVm.filtersActive

                LibraryRail {
                    Layout.fillWidth: true
                    title: i18nc("@title library rail", "Up Next")
                    artworkShape: "thumbnail"
                    model: libraryVm.upNextModel
                    visible: libraryVm.upNextModel
                        && libraryVm.upNextModel.count > 0
                    onItemActivated: row =>
                        libraryVm.activateRail("upNext", row)
                }
                LibraryRail {
                    Layout.fillWidth: true
                    title: i18nc("@title library rail", "Airing soon")
                    artworkShape: "thumbnail"
                    model: libraryVm.airingSoonModel
                    visible: libraryVm.airingSoonModel
                        && libraryVm.airingSoonModel.count > 0
                    onItemActivated: row =>
                        libraryVm.activateRail("airingSoon", row)
                }
                LibraryRail {
                    Layout.fillWidth: true
                    title: i18nc("@title library rail", "Coming up")
                    artworkShape: "poster"
                    model: libraryVm.comingUpModel
                    visible: libraryVm.comingUpModel
                        && libraryVm.comingUpModel.count > 0
                    onItemActivated: row =>
                        libraryVm.activateRail("comingUp", row)
                }
            }

            // Grid sub-heading + Clear filters affordance.
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Theme.pageMargin
                Layout.rightMargin: Theme.pageMargin
                Layout.topMargin: Theme.inlineSpacing

                Kirigami.Heading {
                    level: 3
                    text: libraryVm.filtersActive
                        ? i18nc("@title library results heading",
                            "Filtered (%1)", libraryVm.model.count)
                        : i18nc("@title library results heading",
                            "All titles (%1)", libraryVm.totalCount)
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                }
                QQC2.ToolButton {
                    visible: libraryVm.filtersActive
                    text: i18nc("@action:button reset library filters",
                        "Clear filters")
                    icon.source: AppIcons.url("eraser")
                    icon.color: AppIcons.foreground
                    flat: true
                    onClicked: libraryVm.resetFilters()
                }
            }

            Item { Layout.preferredHeight: Theme.inlineSpacing }
        }

        delegate: Item {
            width: grid.cellWidth
            height: grid.cellHeight

            LibraryCard {
                anchors.fill: parent
                anchors.margins: grid.cellInset
                posterUrl: model.posterUrl !== undefined
                    ? model.posterUrl : ""
                title: model.title !== undefined ? model.title : ""
                subtitle: model.subtitle !== undefined ? model.subtitle : ""
                progress: model.progress !== undefined ? model.progress : -1
                watched: model.watched !== undefined ? model.watched : false
                upcoming: model.upcoming !== undefined ? model.upcoming : false
                releaseDateText: model.releaseDateText !== undefined
                    ? model.releaseDateText : ""
                onClicked: libraryVm.activate(index)
                onRemoveRequested: {
                    removeDialog.targetRow = index;
                    removeDialog.targetTitle = model.title !== undefined
                        ? model.title : "";
                    removeDialog.open();
                }
                onToggleWatchedRequested: libraryVm.toggleWatched(index)
            }
        }
    }
}
