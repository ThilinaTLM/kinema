// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.components as Components

import dev.tlmtech.kinema.app

// Browse: filter-driven TMDB grid. Chrome:
//
//   * `header:` is a `QQC2.ToolBar` (Header colorSet) that pairs a
//     `Components.SegmentedButton` (Movies / TV Series) with a
//     `Kirigami.ActionToolBar` of filter actions: Genres ▾, Released ▾,
//     ★ Min ▾, Sort ▾, and a Hide-obscure toggle. Every filter is a
//     `Kirigami.Action`; `ActionToolBar` collapses overflow into its
//     built-in ▸ menu on narrow widths.
//   * Below the header, an inline chip strip paints removable
//     `Kirigami.Chip`s for every non-default filter, with a trailing
//     "Clear all" button. Auto-collapses to height 0 when no filter is
//     active.
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
            icon.name: "view-refresh"
            text: i18nc("@action:button", "Refresh")
            displayHint: Kirigami.DisplayHint.IconOnly
            shortcut: StandardKey.Refresh
            onTriggered: browseVm.refresh()
        }
    ]

    // ---- header: filter toolbar ---------------------------------
    header: QQC2.ToolBar {
        id: filterBar

        Kirigami.Theme.colorSet: Kirigami.Theme.Header
        Kirigami.Theme.inherit: false

        leftPadding: Theme.pageMargin
        rightPadding: Theme.pageMargin
        topPadding: Theme.inlineSpacing
        bottomPadding: Theme.inlineSpacing

        visible: browseVm.tmdbConfigured && !browseVm.authFailed

        contentItem: RowLayout {
            spacing: Theme.groupSpacing

            // ---- Movies / TV Series ---------------------------------
            Components.SegmentedButton {
                Layout.alignment: Qt.AlignVCenter
                actions: [
                    Kirigami.Action {
                        text: i18nc("@option:radio media kind", "Movies")
                        icon.name: "video-x-generic"
                        checkable: true
                        checked: browseVm.kind === 0
                        onTriggered: if (browseVm.kind !== 0) {
                            browseVm.kind = 0;
                        }
                    },
                    Kirigami.Action {
                        text: i18nc("@option:radio media kind", "TV Series")
                        icon.name: "view-media-recent"
                        checkable: true
                        checked: browseVm.kind === 1
                        onTriggered: if (browseVm.kind !== 1) {
                            browseVm.kind = 1;
                        }
                    }
                ]
            }

            // ---- filter actions (with overflow menu on narrow) -----
            Kirigami.ActionToolBar {
                id: filterToolBar
                Layout.fillWidth: true
                alignment: Qt.AlignLeft
                flat: true

                actions: [
                    // ---- Genres ▾ (multi-select Menu) ---------------
                    Kirigami.Action {
                        id: genresAction
                        // Empty parent action; rendered via displayComponent
                        // because ActionToolBar's children: pattern only
                        // handles single-select sub-menus, and Genres needs
                        // multi-select with a "Clear all" affordance.
                        text: browseVm.genreIds.length > 0
                            ? i18ncp("@action:button genres button with active count",
                                "Genres (%1)", "Genres (%1)",
                                browseVm.genreIds.length)
                            : i18nc("@action:button", "Genres")
                        icon.name: "view-categories"
                        enabled: browseVm.availableGenres.length > 0

                        displayComponent: QQC2.ToolButton {
                            id: genresBtn
                            // Match Kirigami.ActionToolBar's native button
                            // styling so this row aligns with the rest of
                            // the actions (flat, same display mode).
                            flat: true
                            display: filterToolBar.display
                            text: genresAction.text
                            // `Kirigami.Action.icon` is a grouped property;
                            // reading `.name` from outside the action scope
                            // doesn't propagate reliably to the displayed
                            // ToolButton, so hard-code it here. The wrapper
                            // action above keeps `icon.name` set so the
                            // overflow menu renders the icon too.
                            icon.name: "view-categories"
                            enabled: genresAction.enabled
                            checkable: true
                            checked: genresMenu.opened
                            onClicked: genresMenu.opened
                                ? genresMenu.close()
                                : genresMenu.open()

                            QQC2.Menu {
                                id: genresMenu
                                y: genresBtn.height
                                implicitWidth: Math.max(genresBtn.width,
                                    Kirigami.Units.gridUnit * 14)

                                QQC2.MenuItem {
                                    text: i18nc(
                                        "@action:inmenu clear all genre selections",
                                        "Clear all genres")
                                    icon.name: "edit-clear-history"
                                    enabled: browseVm.genreIds.length > 0
                                    onTriggered: browseVm.genreIds = []
                                }
                                QQC2.MenuSeparator { }

                                Repeater {
                                    model: browseVm.availableGenres

                                    delegate: QQC2.MenuItem {
                                        required property var modelData
                                        text: modelData.name !== undefined
                                            ? modelData.name : ""
                                        checkable: true
                                        checked: modelData.checked === true
                                        onTriggered: {
                                            const id = modelData.id;
                                            let next = (browseVm.genreIds || []).slice();
                                            const idx = next.indexOf(id);
                                            if (checked && idx < 0) {
                                                next.push(id);
                                            } else if (!checked && idx >= 0) {
                                                next.splice(idx, 1);
                                            } else {
                                                return;
                                            }
                                            browseVm.genreIds = next;
                                        }
                                    }
                                }
                            }
                        }
                    },

                    // ---- Released ▾ (pick-one sub-menu) -------------
                    Kirigami.Action {
                        text: i18nc("@action:button browse filter", "Released")
                        icon.name: "view-calendar"
                        children: [
                            Kirigami.Action {
                                text: i18nc("@item date window", "Past month")
                                checkable: true
                                checked: browseVm.dateWindow === 0
                                onTriggered: if (browseVm.dateWindow !== 0) {
                                    browseVm.dateWindow = 0;
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@item date window", "Past 3 months")
                                checkable: true
                                checked: browseVm.dateWindow === 1
                                onTriggered: if (browseVm.dateWindow !== 1) {
                                    browseVm.dateWindow = 1;
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@item date window", "This year")
                                checkable: true
                                checked: browseVm.dateWindow === 2
                                onTriggered: if (browseVm.dateWindow !== 2) {
                                    browseVm.dateWindow = 2;
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@item date window", "Past 3 years")
                                checkable: true
                                checked: browseVm.dateWindow === 3
                                onTriggered: if (browseVm.dateWindow !== 3) {
                                    browseVm.dateWindow = 3;
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@item date window", "Any time")
                                checkable: true
                                checked: browseVm.dateWindow === 4
                                onTriggered: if (browseVm.dateWindow !== 4) {
                                    browseVm.dateWindow = 4;
                                }
                            }
                        ]
                    },

                    // ---- ★ Min ▾ (pick-one sub-menu) ----------------
                    // Discrete thresholds matching the VM's step-5 ints.
                    // `minRatingPct` accepts 0..90; we surface the 8
                    // values users actually care about.
                    Kirigami.Action {
                        text: browseVm.minRatingPct <= 0
                            ? i18nc("@action:button browse rating filter", "★ Any")
                            : i18nc("@action:button browse rating filter, %1 is a localized one-decimal rating",
                                "★ %1+",
                                (browseVm.minRatingPct / 10).toFixed(1))
                        icon.name: "rating"
                        children: [
                            Kirigami.Action {
                                text: i18nc("@item rating threshold", "Any")
                                checkable: true
                                checked: browseVm.minRatingPct === 0
                                onTriggered: if (browseVm.minRatingPct !== 0) {
                                    browseVm.minRatingPct = 0;
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@item rating threshold", "★ 5+")
                                checkable: true
                                checked: browseVm.minRatingPct === 50
                                onTriggered: if (browseVm.minRatingPct !== 50) {
                                    browseVm.minRatingPct = 50;
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@item rating threshold", "★ 6+")
                                checkable: true
                                checked: browseVm.minRatingPct === 60
                                onTriggered: if (browseVm.minRatingPct !== 60) {
                                    browseVm.minRatingPct = 60;
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@item rating threshold", "★ 7+")
                                checkable: true
                                checked: browseVm.minRatingPct === 70
                                onTriggered: if (browseVm.minRatingPct !== 70) {
                                    browseVm.minRatingPct = 70;
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@item rating threshold", "★ 7.5+")
                                checkable: true
                                checked: browseVm.minRatingPct === 75
                                onTriggered: if (browseVm.minRatingPct !== 75) {
                                    browseVm.minRatingPct = 75;
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@item rating threshold", "★ 8+")
                                checkable: true
                                checked: browseVm.minRatingPct === 80
                                onTriggered: if (browseVm.minRatingPct !== 80) {
                                    browseVm.minRatingPct = 80;
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@item rating threshold", "★ 8.5+")
                                checkable: true
                                checked: browseVm.minRatingPct === 85
                                onTriggered: if (browseVm.minRatingPct !== 85) {
                                    browseVm.minRatingPct = 85;
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@item rating threshold", "★ 9+")
                                checkable: true
                                checked: browseVm.minRatingPct === 90
                                onTriggered: if (browseVm.minRatingPct !== 90) {
                                    browseVm.minRatingPct = 90;
                                }
                            }
                        ]
                    },

                    // ---- Sort ▾ (pick-one sub-menu) -----------------
                    Kirigami.Action {
                        text: i18nc("@action:button browse sort", "Sort")
                        icon.name: "view-sort"
                        children: [
                            Kirigami.Action {
                                text: i18nc("@item sort", "Most popular")
                                checkable: true
                                checked: browseVm.sort === 0
                                onTriggered: if (browseVm.sort !== 0) {
                                    browseVm.sort = 0;
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@item sort", "Newest first")
                                checkable: true
                                checked: browseVm.sort === 1
                                onTriggered: if (browseVm.sort !== 1) {
                                    browseVm.sort = 1;
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@item sort", "Highest rated")
                                checkable: true
                                checked: browseVm.sort === 2
                                onTriggered: if (browseVm.sort !== 2) {
                                    browseVm.sort = 2;
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@item sort", "Title (A\u2013Z)")
                                checkable: true
                                checked: browseVm.sort === 3
                                onTriggered: if (browseVm.sort !== 3) {
                                    browseVm.sort = 3;
                                }
                            }
                        ]
                    },

                    // ---- Hide obscure (toggle) ----------------------
                    Kirigami.Action {
                        text: i18nc("@option:check browse filter", "Hide obscure")
                        icon.name: "view-filter"
                        checkable: true
                        checked: browseVm.hideObscure
                        tooltip: i18nc("@info:tooltip",
                            "Skip results with fewer than 200 ratings.")
                        onTriggered: browseVm.hideObscure = !browseVm.hideObscure
                    }
                ]
            }
        }
    }

    // ---- not-configured / auth-failed placeholder ----------------
    Kirigami.PlaceholderMessage {
        id: placeholder
        anchors.centerIn: parent
        width: Math.min(parent.width - Theme.pageWideMargin * 2,
            Theme.placeholderMaxWidth)
        visible: !browseVm.tmdbConfigured || browseVm.authFailed

        icon.name: browseVm.authFailed
            ? "dialog-warning" : "configure"
        text: browseVm.authFailed
            ? i18nc("@info placeholder",
                "TMDB rejected the token")
            : i18nc("@info placeholder",
                "Set up TMDB to enable Browse")
        explanation: browseVm.authFailed
            ? i18nc("@info placeholder",
                "The bundled TMDB token may have been revoked, or "
                + "your override is invalid. Open Settings → TMDB "
                + "(Discover) to paste a working token.")
            : i18nc("@info placeholder",
                "Browse uses TMDB's /discover endpoint to filter "
                + "the catalog. Open Settings → TMDB (Discover) "
                + "and paste a v4 Read Access Token.")
        helpfulAction: Kirigami.Action {
            icon.name: "settings-configure"
            text: i18nc("@action:button", "Open settings…")
            onTriggered: mainController.requestSettings()
        }
    }

    // ---- content column ------------------------------------------
    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        visible: browseVm.tmdbConfigured && !browseVm.authFailed

        // ---- active-filter chip strip ----------------------------
        // Collapses to zero height when no filter is active so the
        // grid takes the full available area in the common case.
        Item {
            id: chipStrip

            Layout.fillWidth: true
            visible: browseVm.activeChips.length > 0
            implicitHeight: visible
                ? chipRow.implicitHeight + Theme.inlineSpacing
                : 0

            Behavior on implicitHeight {
                NumberAnimation { duration: Kirigami.Units.shortDuration }
            }

            RowLayout {
                id: chipRow
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                anchors.leftMargin: Theme.pageMargin
                anchors.rightMargin: Theme.pageMargin
                spacing: Theme.inlineSpacing

                QQC2.Label {
                    text: i18nc("@label prefix for active filter chip list",
                        "Filters:")
                    color: Kirigami.Theme.disabledTextColor
                }

                QQC2.ScrollView {
                    Layout.fillWidth: true
                    Layout.preferredHeight: chipFlow.implicitHeight
                    contentWidth: chipFlow.implicitWidth
                    contentHeight: chipFlow.implicitHeight
                    QQC2.ScrollBar.horizontal.policy: QQC2.ScrollBar.AsNeeded
                    QQC2.ScrollBar.vertical.policy: QQC2.ScrollBar.AlwaysOff
                    clip: true

                    Row {
                        id: chipFlow
                        spacing: Theme.inlineSpacing

                        Repeater {
                            model: browseVm.activeChips

                            delegate: Kirigami.Chip {
                                required property int index
                                required property var modelData
                                text: modelData.label !== undefined
                                    ? modelData.label : ""
                                closable: true
                                checkable: false
                                Accessible.name: i18nc("@info:whatsthis",
                                    "Remove filter %1", text)
                                onRemoved: browseVm.removeChip(index)
                            }
                        }
                    }
                }

                QQC2.ToolButton {
                    text: i18nc("@action:button clear every active filter",
                        "Clear all")
                    icon.name: "edit-clear-history"
                    display: QQC2.AbstractButton.TextBesideIcon
                    onClicked: browseVm.resetFilters()
                }
            }
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true

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
                        icon.name: "go-down"
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
                icon.name: "edit-find"
                text: i18nc("@info placeholder", "No matches")
                explanation: i18nc("@info placeholder",
                    "Nothing matches these filters. Try widening "
                    + "the date range or clearing some genres.")
                helpfulAction: Kirigami.Action {
                    icon.name: "edit-clear-history"
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
                icon.name: "dialog-error"
                text: i18nc("@info placeholder", "Browse failed")
                explanation: browseVm.results
                    ? browseVm.results.errorMessage : ""
                helpfulAction: Kirigami.Action {
                    icon.name: "view-refresh"
                    text: i18nc("@action:button", "Retry")
                    onTriggered: browseVm.refresh()
                }
            }
        }
    }
}
