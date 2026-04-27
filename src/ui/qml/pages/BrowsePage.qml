// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Browse: filter-driven TMDB grid. Chrome:
//
//   * `header:` is `BrowseFilterBar` — segmented Movies / TV Series
//     toggle, genre chip strip, Released combo, ★ Min rating popup,
//     sort combo, and a Hide-obscure toggle. Every filter lives
//     inline; there is no overflow sheet.
//   * `ActiveFiltersBar` sits between the header and the grid and
//     paints removable chips for every non-default filter, with a
//     trailing "Clear all" button.
//   * Body is the responsive `PosterGrid` over `browseVm.results`,
//     swapped out via `StackLayout` for the loading / empty / error
//     placeholders.
//   * The not-configured / auth-failed `Kirigami.PlaceholderMessage`
//     branch covers the entire page when TMDB isn't usable.
//
// `Show all →` from a Discover rail still lands here via
// `MainController::navigateToBrowseRequested` after applying the
// preset; the chip row is the user-visible record of what's active.
Kirigami.ScrollablePage {
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

    header: BrowseFilterBar {
        visible: browseVm.tmdbConfigured && !browseVm.authFailed
        // Collapse to zero height when hidden so the placeholder
        // takes the whole page surface.
        Layout.preferredHeight: visible ? implicitHeight : 0
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

        ActiveFiltersBar {
            Layout.fillWidth: true
            chips: browseVm.activeChips
            onChipRemoved: function (idx) { browseVm.removeChip(idx); }
            onClearAllRequested: browseVm.resetFilters()
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
