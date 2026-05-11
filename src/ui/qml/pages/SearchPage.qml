// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Cinemeta search page. The page header is a `QQC2.ToolBar` with
// the `MediaKindSelect` (Movies / TV Series) toggle on the left
// followed by a fill-width `Kirigami.SearchField`. The body renders
// one of five surfaces (idle / loading / results / empty / error)
// chosen by `searchVm.results.state`.
//
// `focusSearchField()` is the hook `ApplicationShell.qml`'s
// `onFocusSearchRequested` calls; declaring it here means the
// `Ctrl+F` shortcut focuses the existing field instead of pushing
// a fresh page when the user is already on Search.
// The body's results state is a `PosterGrid` (a GridView) that handles
// its own flicking, so the outer page is a plain `Kirigami.Page`.
// Wrapping a self-flickable view in `Kirigami.ScrollablePage` produces
// a parent Flickable that intercepts wheel/flick events without ever
// scrolling.
Kirigami.Page {
    id: page

    title: i18nc("@title:window", "Search")
    objectName: "search"

    leftPadding: 0
    rightPadding: 0
    topPadding: 0
    bottomPadding: 0

    function focusSearchField() {
        if (searchField) {
            searchField.forceActiveFocus();
            searchField.selectAll();
        }
    }

    Component.onCompleted: focusSearchField()

    header: PageHeaderBar {
        title: page.title

        Item { Layout.fillWidth: true }

        MediaKindSelect {
            Layout.alignment: Qt.AlignVCenter
            kind: searchVm.kind
            onActivated: newKind => searchVm.kind = newKind
        }

        Kirigami.SearchField {
            id: searchField
            Layout.alignment: Qt.AlignVCenter
            Layout.fillWidth: true
            Layout.minimumWidth: Kirigami.Units.gridUnit * 16
            Layout.preferredWidth: Theme.wideContentMaxWidth
            Layout.maximumWidth: Theme.wideContentMaxWidth
            placeholderText: i18nc("@info:placeholder",
                "Search Cinemeta — title or IMDB id (ttXXXXXXX)")
            text: searchVm.query
            // SearchViewModel owns the network debounce; disable
            // SearchField's shorter auto-accept timer so it does
            // not bypass that debounce and submit duplicates.
            autoAccept: false
            onTextEdited: searchVm.query = text
            onAccepted: {
                if (searchVm.query !== text) {
                    searchVm.query = text;
                }
                searchVm.submit();
            }
            Keys.onEscapePressed: function (event) {
                if (text.length > 0) {
                    searchVm.clear();
                    event.accepted = true;
                } else {
                    event.accepted = false;
                }
            }
        }
    }

    // ---- body: state-switched surface --------------------------
    StackLayout {
        anchors.fill: parent
        // ResultsListModel.State enum order matches these indices.
        currentIndex: searchVm.results
            ? searchVm.results.state
            : 0

        // 0 — Idle: no query yet.
        Item {
            Kirigami.PlaceholderMessage {
                anchors.centerIn: parent
                width: Math.min(parent.width - Theme.pageWideMargin * 2,
                    Theme.placeholderMaxWidth)
                icon.source: AppIcons.url("search")
                icon.color: AppIcons.foreground
                text: i18nc("@info placeholder",
                    "Find something to watch")
                explanation: i18nc("@info placeholder",
                    "Type a title or paste an IMDB id (ttXXXXXXX).")
            }
        }

        // 1 — Loading.
        Item {
            QQC2.BusyIndicator {
                anchors.centerIn: parent
                running: true
            }
        }

        // 2 — Results.
        PosterGrid {
            sourceModel: searchVm.results
            onItemActivated: function (row) { searchVm.activate(row); }
        }

        // 3 — Empty.
        Item {
            Kirigami.PlaceholderMessage {
                anchors.centerIn: parent
                width: Math.min(parent.width - Theme.pageWideMargin * 2,
                    Theme.placeholderMaxWidth)
                icon.source: AppIcons.url("search")
                icon.color: AppIcons.foreground
                text: i18nc("@info placeholder", "No results")
                explanation: i18nc("@info placeholder",
                    "Cinemeta returned no matches for this query. "
                    + "Try a different spelling, or browse the catalog.")
                helpfulAction: Kirigami.Action {
                    icon.source: AppIcons.url("grid-2x2")
                    icon.color: AppIcons.foreground
                    text: searchVm.kind === 0
                        ? i18nc("@action:button",
                            "Browse movies instead")
                        : i18nc("@action:button",
                            "Browse TV series instead")
                    onTriggered: shell.applyBrowsePreset(
                        searchVm.kind, 0)
                }
            }
        }

        // 4 — Error.
        Item {
            Kirigami.PlaceholderMessage {
                anchors.centerIn: parent
                width: Math.min(parent.width - Theme.pageWideMargin * 2,
                    Theme.placeholderMaxWidth)
                icon.source: AppIcons.url("circle-alert")
                icon.color: AppIcons.negative
                text: i18nc("@info placeholder", "Search failed")
                explanation: searchVm.results
                    ? searchVm.results.errorMessage
                    : ""
                helpfulAction: Kirigami.Action {
                    icon.source: AppIcons.url("refresh-cw")
                    icon.color: AppIcons.foreground
                    text: i18nc("@action:button", "Retry")
                    onTriggered: searchVm.submit()
                }
            }
        }
    }
}
