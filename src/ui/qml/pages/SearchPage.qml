// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Cinemeta search page. The header carries a `Kirigami.SearchField`
// + a Movies/Series segmented control bound to `searchVm`. Below
// it, the body renders one of five surfaces (idle / loading /
// results / empty / error) chosen by `searchVm.results.state`.
//
// `focusSearchField()` is the hook `ApplicationShell.qml`'s
// `onFocusSearchRequested` looks for; declaring it here means the
// `Ctrl+F` shortcut focuses the existing field instead of pushing
// a fresh page when the user is already on Search.
Kirigami.ScrollablePage {
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

    header: Kirigami.AbstractApplicationHeader {
        // The default Kirigami header height plus a smidge for the
        // segmented control, so two-line wrapping doesn't clip the
        // input on Plasma's compact font sizes.
        Layout.preferredHeight: contentRow.implicitHeight
            + Kirigami.Units.largeSpacing * 2

        contentItem: RowLayout {
            id: contentRow
            spacing: Kirigami.Units.largeSpacing
            anchors.leftMargin: Kirigami.Units.largeSpacing
            anchors.rightMargin: Kirigami.Units.largeSpacing

            Kirigami.SearchField {
                id: searchField
                Layout.fillWidth: true
                placeholderText: i18nc("@info:placeholder",
                    "Search Cinemeta — title or IMDB id (ttXXXXXXX)")
                text: searchVm.query
                onTextEdited: searchVm.query = text
                onAccepted: searchVm.submit()
                Keys.onEscapePressed: function (event) {
                    if (text.length > 0) {
                        searchVm.clear();
                        event.accepted = true;
                    } else {
                        event.accepted = false;
                    }
                }
            }

            MediaKindSwitch {
                kind: searchVm.kind
                onActivated: function (newKind) {
                    searchVm.kind = newKind;
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
        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: Math.min(parent.width
                    - Kirigami.Units.gridUnit * 4,
                Kirigami.Units.gridUnit * 28)
            icon.name: "search"
            text: i18nc("@info placeholder",
                "Type a title or paste an IMDB id (ttXXXXXXX) "
                + "and press Enter.")
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
        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: Math.min(parent.width
                    - Kirigami.Units.gridUnit * 4,
                Kirigami.Units.gridUnit * 28)
            icon.name: "edit-find"
            text: i18nc("@info placeholder", "No results")
            explanation: i18nc("@info placeholder",
                "Cinemeta returned no matches for this query. "
                + "Try a different spelling or switch the kind toggle.")
        }

        // 4 — Error.
        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: Math.min(parent.width
                    - Kirigami.Units.gridUnit * 4,
                Kirigami.Units.gridUnit * 28)
            icon.name: "dialog-error"
            text: i18nc("@info placeholder", "Search failed")
            explanation: searchVm.results
                ? searchVm.results.errorMessage
                : ""
            helpfulAction: Kirigami.Action {
                icon.name: "view-refresh"
                text: i18nc("@action:button", "Retry")
                onTriggered: searchVm.submit()
            }
        }
    }
}
