// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

// Browse: filter-driven TMDB grid. Chrome:
//
//   * `header:` is a single merged `PageHeaderBar` that inlines the
//     page title with the basic filters (`MediaKindSelect`,
//     `Genres ▾`, `Sort ▾`) and a `More filters… (N)` button. The
//     advanced dialog (`browseAdvancedDialog`) carries `Released`,
//     `★ Min`, and the `Hide obscure` toggle — every uncommon filter
//     lives behind that one button so the inline bar fits at narrow
//     widths without overflowing.
//   * Below the header, an inline chip strip paints removable
//     `Kirigami.Chip`s for every non-default filter (basic and
//     advanced), with a trailing "Clear all" button. Auto-collapses
//     to height 0 when no filter is active.
//   * Body is the responsive `PosterGrid` over `browseVm.results`,
//     swapped out via `StackLayout` for the loading / empty / error
//     placeholders.
//   * The not-configured / auth-failed `Kirigami.PlaceholderMessage`
//     branch covers the entire page when TMDB isn't usable.
//
// `Show all →` from a Discover rail still lands here via
// `MainController::navigateToBrowseRequested` after applying the
// preset; the chip row is the user-visible record of what's active.
//
// Browse uses an inner `PosterGrid` (a GridView) that handles its own
// flicking, so the outer page is a plain `Kirigami.Page`. Wrapping a
// self-flickable view in `Kirigami.ScrollablePage` produces a parent
// Flickable that intercepts wheel/flick events without ever scrolling.
Kirigami.Page {
    id: page

    title: i18nc("@title:window", "Browse")
    objectName: "browse"

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
            onTriggered: browseVm.refresh()
        }
    ]

    // ---- advanced filters dialog --------------------------------
    // Released window, minimum rating, and the "Hide obscure" toggle
    // live behind "More filters…" so the inline header stays at
    // three controls (kind / Genres / Sort) and fits at narrow widths.
    Kirigami.Dialog {
        id: browseAdvancedDialog
        title: i18nc("@title:dialog browse advanced filters",
            "More filters")

        footer: QQC2.DialogButtonBox {
            QQC2.Button {
                text: i18nc("@action:button reset all browse filters",
                    "Reset all filters")
                icon.source: AppIcons.url("eraser")
                icon.color: AppIcons.foreground
                visible: browseVm.dateWindow !== 3
                    || browseVm.minRatingPct > 0
                    || browseVm.hideObscure
                    || browseVm.genreIds.length > 0
                    || browseVm.sort !== 0
                QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.ActionRole
                onClicked: {
                    browseVm.resetFilters();
                    browseAdvancedDialog.close();
                }
            }
            QQC2.Button {
                text: i18nc("@action:button close dialog", "Close")
                QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.AcceptRole
                onClicked: browseAdvancedDialog.close()
            }
        }
        preferredWidth: Kirigami.Units.gridUnit * 26

        FormCard.FormCard {
            FormCard.FormComboBoxDelegate {
                text: i18nc("@label browse filter", "Released")
                model: [
                    i18nc("@item date window", "Past month"),
                    i18nc("@item date window", "Past 3 months"),
                    i18nc("@item date window", "This year"),
                    i18nc("@item date window", "Past 3 years"),
                    i18nc("@item date window", "Any time")
                ]
                currentIndex: browseVm.dateWindow
                onActivated: idx => browseVm.dateWindow = idx
            }
            FormCard.FormComboBoxDelegate {
                text: i18nc("@label browse filter",
                    "Minimum rating")
                // Index <-> minRatingPct map. The view-model keeps
                // discrete step-5 ints; the UI surfaces the eight
                // values users actually care about.
                readonly property var pctValues: [0, 50, 60, 70, 75, 80, 85, 90]
                model: [
                    i18nc("@item rating threshold", "Any"),
                    i18nc("@item rating threshold", "★ 5+"),
                    i18nc("@item rating threshold", "★ 6+"),
                    i18nc("@item rating threshold", "★ 7+"),
                    i18nc("@item rating threshold", "★ 7.5+"),
                    i18nc("@item rating threshold", "★ 8+"),
                    i18nc("@item rating threshold", "★ 8.5+"),
                    i18nc("@item rating threshold", "★ 9+")
                ]
                currentIndex: Math.max(0,
                    pctValues.indexOf(browseVm.minRatingPct))
                onActivated: idx => {
                    const v = pctValues[idx];
                    if (browseVm.minRatingPct !== v) {
                        browseVm.minRatingPct = v;
                    }
                }
            }
            FormCard.FormSwitchDelegate {
                text: i18nc("@option:check browse filter",
                    "Hide obscure")
                description: i18nc("@info",
                    "Skip results with fewer than 200 ratings.")
                checked: browseVm.hideObscure
                onToggled: browseVm.hideObscure = checked
            }
        }
    }

    // ---- header: merged title + filter bar ----------------------
    header: PageHeaderBar {
        id: filterBar
        title: page.title
        visible: browseVm.tmdbConfigured && !browseVm.authFailed
        pageActions: page.actions
        advancedFiltersDialog: browseAdvancedDialog
        // Count advanced filters that deviate from defaults.
        // dateWindow default = Past 3 years (index 3), minRatingPct
        // default = 0, hideObscure default = false.
        advancedFilterCount:
            (browseVm.dateWindow !== 3 ? 1 : 0)
            + (browseVm.minRatingPct > 0 ? 1 : 0)
            + (browseVm.hideObscure ? 1 : 0)

        // Right-align filter controls after the title.
        Item { Layout.fillWidth: true }

        // ---- Movies / TV Series ---------------------------------
        MediaKindSelect {
            Layout.alignment: Qt.AlignVCenter
            kind: browseVm.kind
            onActivated: newKind => browseVm.kind = newKind
        }

        // ---- Genres (multi-select) ------------------------------
        FilterMenuButton {
            Layout.alignment: Qt.AlignVCenter
            axisLabel: i18nc("@action:button browse filter", "Genres")
            icon.source: AppIcons.url("tags")
            multiSelect: true
            active: browseVm.genreIds.length > 0
            enabled: browseVm.availableGenres.length > 0
            options: {
                const out = [];
                const src = browseVm.availableGenres || [];
                for (let i = 0; i < src.length; ++i) {
                    out.push({
                        value: src[i].id,
                        label: src[i].name !== undefined
                            ? src[i].name : ""
                    });
                }
                return out;
            }
            currentValues: browseVm.genreIds
            onMultiActivated: list => browseVm.genreIds = list
        }

        // ---- Sort (pick-one) ------------------------------------
        FilterMenuButton {
            Layout.alignment: Qt.AlignVCenter
            axisLabel: i18nc("@action:button browse sort", "Sort")
            icon.source: AppIcons.url("arrow-up-down")
            active: browseVm.sort !== 0
            options: [
                { value: 0, label:
                    i18nc("@item sort", "Most popular") },
                { value: 1, label:
                    i18nc("@item sort", "Newest first") },
                { value: 2, label:
                    i18nc("@item sort", "Highest rated") },
                { value: 3, label:
                    i18nc("@item sort", "Title (A–Z)") }
            ]
            currentValue: browseVm.sort
            onActivated: v => browseVm.sort = v
        }
    }

    // ---- not-configured / auth-failed placeholder ----------------
    Kirigami.PlaceholderMessage {
        id: placeholder
        anchors.centerIn: parent
        width: Math.min(parent.width - Theme.pageWideMargin * 2,
            Theme.placeholderMaxWidth)
        visible: !browseVm.tmdbConfigured || browseVm.authFailed

        icon.source: AppIcons.url(browseVm.authFailed
            ? "triangle-alert" : "settings")
        icon.color: browseVm.authFailed ? AppIcons.negative : AppIcons.foreground
        text: browseVm.authFailed
            ? i18nc("@info placeholder",
                "TMDB rejected the token")
            : i18nc("@info placeholder",
                "Set up TMDB to enable Browse")
        explanation: browseVm.authFailed
            ? i18nc("@info placeholder",
                "The bundled TMDB token may have been revoked, or "
                + "your override is invalid. Open Settings → TMDB "
                + "to paste a working token.")
            : i18nc("@info placeholder",
                "Browse uses TMDB's /discover endpoint to filter "
                + "the catalog. Open Settings → TMDB and paste a "
                + "v4 Read Access Token.")
        helpfulAction: Kirigami.Action {
            icon.source: AppIcons.url("settings")
            icon.color: AppIcons.foreground
            text: i18nc("@action:button", "Open settings…")
            onTriggered: mainController.requestSettings()
        }
    }

    // ---- content -------------------------------------------------
    StackLayout {
        anchors.fill: parent
        visible: browseVm.tmdbConfigured && !browseVm.authFailed

        // Switch between [content], [loading], [empty], [error]
        // by inspecting the result model's state. We reuse the
        // DiscoverSectionModel state enum since BrowseVM's
        // results list is an instance of it.
        currentIndex: {
            if (!browseVm.results) return 0;
            switch (browseVm.results.state) {
            case DiscoverSectionModel.Loading:
                return 1;
            case DiscoverSectionModel.Empty:
                return 2;
            case DiscoverSectionModel.Error:
                return 3;
            case DiscoverSectionModel.Ready:
            default:
                return 0;
            }
        }

            // 0 — Results grid (+ optional load-more sentinel).
            ColumnLayout {
                spacing: 0

                PosterGrid {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    sourceModel: browseVm.results
                    onItemActivated: function (row) {
                        browseVm.activate(row);
                    }
                    onNearEndOfList: {
                        if (browseVm.canLoadMore) {
                            browseVm.loadMore();
                        }
                    }
                }

                // Inline footer status when paginating.
                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: Theme.inlineSpacing
                    visible: browseVm.canLoadMore || browseVm.loading

                    Item { Layout.fillWidth: true }
                    QQC2.BusyIndicator {
                        running: browseVm.loading
                        visible: browseVm.loading
                        Layout.preferredWidth: Kirigami.Units.iconSizes.small
                        Layout.preferredHeight: Kirigami.Units.iconSizes.small
                    }
                    QQC2.Button {
                        text: i18nc("@action:button browse pagination",
                            "Load more")
                        icon.source: AppIcons.url("chevrons-down")
                        icon.color: AppIcons.controlColor(enabled, false)
                        visible: browseVm.canLoadMore
                            && !browseVm.loading
                        onClicked: browseVm.loadMore()
                    }
                    Item { Layout.fillWidth: true }
                }
            }

            // 1 — Loading placeholder.
            Item {
                QQC2.BusyIndicator {
                    anchors.centerIn: parent
                    running: true
                }
            }

            // 2 — Empty.
            Kirigami.PlaceholderMessage {
                Layout.alignment: Qt.AlignCenter
                Layout.preferredWidth: Math.min(parent.width
                        - Theme.pageWideMargin * 2,
                    Theme.placeholderMaxWidth)
                icon.source: AppIcons.url("search")
                icon.color: AppIcons.foreground
                text: i18nc("@info placeholder", "No matches")
                explanation: i18nc("@info placeholder",
                    "Nothing matches these filters. Try widening "
                    + "the date range or clearing some genres.")
                helpfulAction: Kirigami.Action {
                    icon.source: AppIcons.url("eraser")
                    icon.color: AppIcons.foreground
                    text: i18nc("@action:button", "Reset filters")
                    onTriggered: browseVm.resetFilters()
                }
            }

            // 3 — Error.
            Kirigami.PlaceholderMessage {
                Layout.alignment: Qt.AlignCenter
                Layout.preferredWidth: Math.min(parent.width
                        - Theme.pageWideMargin * 2,
                    Theme.placeholderMaxWidth)
                icon.source: AppIcons.url("circle-alert")
                icon.color: AppIcons.negative
                text: i18nc("@info placeholder", "Browse failed")
                explanation: browseVm.results
                    ? browseVm.results.errorMessage : ""
            helpfulAction: Kirigami.Action {
                    icon.source: AppIcons.url("refresh-cw")
                    icon.color: AppIcons.foreground
                    text: i18nc("@action:button", "Retry")
                    onTriggered: browseVm.refresh()
                }
            }
    }
}
