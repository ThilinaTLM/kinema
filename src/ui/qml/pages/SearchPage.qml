// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Cinemeta search page. The page header is a `QQC2.ToolBar` with a
// large `Kirigami.SearchField` and the segmented `MediaKindSwitch`
// alongside it; below the field a small hint subline explains the
// IMDB-id shortcut. The body renders one of five surfaces (idle /
// loading / results / empty / error) chosen by `searchVm.results.state`.
//
// `focusSearchField()` is the hook `ApplicationShell.qml`'s
// `onFocusSearchRequested` calls; declaring it here means the
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

    header: QQC2.ToolBar {
        Kirigami.Theme.colorSet: Kirigami.Theme.Header
        Kirigami.Theme.inherit: false

        contentItem: ColumnLayout {
            spacing: Kirigami.Units.smallSpacing

            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                Layout.topMargin: Kirigami.Units.smallSpacing
                spacing: Kirigami.Units.largeSpacing

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

            QQC2.Label {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                Layout.bottomMargin: Kirigami.Units.smallSpacing
                visible: searchVm.query.length === 0
                text: i18nc("@info:hint search bar",
                    "Tip: paste an IMDB id (ttXXXXXXX) for a direct lookup, "
                    + "or press Esc to clear.")
                font: Kirigami.Theme.smallFont
                color: Kirigami.Theme.disabledTextColor
                elide: Text.ElideRight
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
            Layout.alignment: Qt.AlignCenter
            Layout.preferredWidth: Math.min(parent.width
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
            Layout.alignment: Qt.AlignCenter
            Layout.preferredWidth: Math.min(parent.width
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
            Layout.alignment: Qt.AlignCenter
            Layout.preferredWidth: Math.min(parent.width
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
