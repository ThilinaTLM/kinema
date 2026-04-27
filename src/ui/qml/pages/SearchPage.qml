// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Cinemeta search page. The page header is a `QQC2.ToolBar` with
// the segmented `MediaKindSwitch` on the left followed by a
// fill-width `Kirigami.SearchField`. The body renders one of five
// surfaces (idle / loading / results / empty / error) chosen by
// `searchVm.results.state`. Idle additionally shows a recent-
// searches strip from `SearchSettings`.
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

        contentItem: RowLayout {
            spacing: Kirigami.Units.largeSpacing

            MediaKindSwitch {
                kind: searchVm.kind
                onActivated: function (newKind) {
                    searchVm.kind = newKind;
                }
            }

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
        }
    }

    // ---- body: state-switched surface --------------------------
    StackLayout {
        anchors.fill: parent
        // ResultsListModel.State enum order matches these indices.
        currentIndex: searchVm.results
            ? searchVm.results.state
            : 0

        // 0 — Idle: no query yet. Centred placeholder + a
        // recent-searches strip when there is history.
        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing

            Item { Layout.fillHeight: true }

            Kirigami.PlaceholderMessage {
                Layout.alignment: Qt.AlignHCenter
                Layout.preferredWidth: Math.min(
                    page.width - Kirigami.Units.gridUnit * 4,
                    Kirigami.Units.gridUnit * 28)
                icon.name: "search"
                text: i18nc("@info placeholder",
                    "Find something to watch")
                explanation: i18nc("@info placeholder",
                    "Type a title or paste an IMDB id (ttXXXXXXX).")
            }

            // Recent searches — only shown when history exists.
            ColumnLayout {
                visible: searchVm.recentQueries.length > 0
                Layout.alignment: Qt.AlignHCenter
                Layout.preferredWidth: Math.min(
                    page.width - Kirigami.Units.gridUnit * 4,
                    Kirigami.Units.gridUnit * 36)
                spacing: Kirigami.Units.smallSpacing

                RowLayout {
                    Layout.fillWidth: true
                    QQC2.Label {
                        Layout.fillWidth: true
                        text: i18nc("@label", "Recent searches")
                        font: Kirigami.Theme.smallFont
                        color: Kirigami.Theme.disabledTextColor
                    }
                    QQC2.ToolButton {
                        text: i18nc("@action:button",
                            "Clear recent searches")
                        icon.name: "edit-clear-history"
                        display: QQC2.AbstractButton.IconOnly
                        QQC2.ToolTip.text: text
                        QQC2.ToolTip.visible: hovered
                        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                        onClicked: searchVm.clearRecent()
                    }
                }

                Flow {
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.smallSpacing

                    Repeater {
                        model: searchVm.recentQueries
                        delegate: Kirigami.Chip {
                            required property string modelData
                            text: modelData
                            checkable: false
                            closable: false
                            onClicked: searchVm.useRecent(modelData)
                        }
                    }
                }
            }

            Item { Layout.fillHeight: true }
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
                + "Try a different spelling, or browse the catalog.")
            helpfulAction: Kirigami.Action {
                icon.name: "view-list-details"
                text: searchVm.kind === 0
                    ? i18nc("@action:button",
                        "Browse movies instead")
                    : i18nc("@action:button",
                        "Browse TV series instead")
                onTriggered: mainController.applyBrowsePreset(
                    searchVm.kind, 0)
            }
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
