// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import dev.tlmtech.kinema.player

import "../components"

/**
 * On-demand "Download subtitle…" sheet. Opens from the
 * `SubtitlePicker`'s footer; lists the current
 * `playerVm.subtitleSearchModel` rows with badges for
 * moviehash-match (🎯), already-cached (⏬), and active-in-mpv (✓).
 *
 * Search filters live in the header; submitting fires
 * `playerVm.requestSubtitleSearch(...)` which `MainWindow` routes to
 * `SubtitleController::runQuery`. Clicking a row fires
 * `playerVm.requestSubtitleDownload(fileId)`.
 */
Popup {
    id: root

    modal: true
    padding: 0
    // Responsive: cap at ~34 grid units, never wider than the scene
    // less standard chrome margins.
    width: Math.min(Theme.gridUnit * 34,
        parent ? parent.width - Theme.spacingLg * 2 : Theme.gridUnit * 34)
    height: Math.min(Theme.gridUnit * 40,
        parent ? parent.height * 0.85 : Theme.gridUnit * 40)

    // Below the compact breakpoint the form grid collapses to one
    // column so labels stack above their inputs instead of clipping.
    readonly property bool isCompact: width < Theme.compactBreakpoint

    background: Item {}

    // Allow PlayerScene to drive open state so we can keep the chrome
    // visible while the sheet is up.
    readonly property bool sheetOpen: opened

    // Default filter values; overwritten by C++ before opening when
    // preferred languages / HI / FPO defaults are configured.
    property var initialLanguages: []
    property string initialHi: "off"
    property string initialFpo: "off"

    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.fadeMs }
            NumberAnimation { property: "scale";   from: 0.98; to: 1; duration: Theme.fadeMs }
        }
    }
    exit: Transition {
        NumberAnimation { property: "opacity"; from: 1; to: 0; duration: Theme.fadeMs }
    }

    // Run a search whenever the sheet opens so the user sees the
    // current set of hits even if the filter inputs are unchanged.
    onOpenedChanged: if (opened) submit()

    function submit() {
        const langs = (langField.text || "").split(",")
            .map(s => s.trim()).filter(s => s.length === 3);
        playerVm.requestSubtitleSearch(langs,
            hiCombo.currentValue,
            fpoCombo.currentValue,
            releaseField.text);
    }

    contentItem: PopupPanel {
        title: qsTr("Download subtitle")
        onCloseRequested: root.close()

        ColumnLayout {
            anchors.fill: parent
            spacing: Theme.spacing

            // ---- Filter row ------------------------------------------
            GridLayout {
                id: filterGrid
                Layout.fillWidth: true
                columns: root.isCompact ? 1 : 2
                columnSpacing: root.isCompact ? 0 : Theme.spacing
                rowSpacing: Theme.spacingSm

                Label { text: qsTr("Languages:") ; color: Theme.foregroundDim }
                TextField {
                    id: langField
                    Layout.fillWidth: true
                    placeholderText: qsTr("eng, spa, fre")
                    text: (root.initialLanguages || []).join(", ")
                    onAccepted: root.submit()
                }

                Label { text: qsTr("Hearing impaired:"); color: Theme.foregroundDim }
                ComboBox {
                    id: hiCombo
                    Layout.fillWidth: true
                    textRole: "label"
                    valueRole: "value"
                    model: [
                        { label: qsTr("Off"),     value: "off" },
                        { label: qsTr("Include"), value: "include" },
                        { label: qsTr("Only"),    value: "only" },
                    ]
                    currentIndex: { switch (root.initialHi) {
                        case "include": return 1;
                        case "only": return 2;
                        default: return 0;
                    } }
                }

                Label { text: qsTr("Foreign parts only:"); color: Theme.foregroundDim }
                ComboBox {
                    id: fpoCombo
                    Layout.fillWidth: true
                    textRole: "label"
                    valueRole: "value"
                    model: [
                        { label: qsTr("Off"),     value: "off" },
                        { label: qsTr("Include"), value: "include" },
                        { label: qsTr("Only"),    value: "only" },
                    ]
                    currentIndex: { switch (root.initialFpo) {
                        case "include": return 1;
                        case "only": return 2;
                        default: return 0;
                    } }
                }

                Label { text: qsTr("Release filter:"); color: Theme.foregroundDim }
                TextField {
                    id: releaseField
                    Layout.fillWidth: true
                    placeholderText: qsTr("e.g. 1080p, BluRay, REMUX")
                    onAccepted: root.submit()
                }

                // Spacer for the second column in 2-col mode; hidden
                // in compact mode so the Search button hugs the grid.
                Item { visible: !root.isCompact }
                Button {
                    Layout.alignment: Qt.AlignRight
                    text: qsTr("Search")
                    onClicked: root.submit()
                }
            }

            Rectangle {
                Layout.fillWidth: true
                height: 1
                color: Theme.border
            }

            // ---- Status ----------------------------------------------
            Label {
                Layout.fillWidth: true
                text: playerVm.subtitleSearchError
                visible: text.length > 0
                color: Theme.danger
                wrapMode: Text.WordWrap
            }
            BufferingIndicator {
                anchors.horizontalCenter: parent.horizontalCenter
                active: playerVm.subtitleSearchActive
                visible: active
            }

            // ---- Results list ----------------------------------------
            ListView {
                id: list
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                spacing: 2
                model: playerVm.subtitleSearchModel
                boundsBehavior: Flickable.StopAtBounds

                delegate: Item {
                    width: ListView.view ? ListView.view.width : 0
                    height: rowCol.implicitHeight + 2 * Theme.spacingSm

                    Rectangle {
                        anchors.fill: parent
                        radius: Theme.radius
                        color: hover.hovered ? Theme.hoverFill : "transparent"
                        border.width: hover.hovered ? 1 : 0
                        border.color: hover.hovered ? Theme.accent : "transparent"
                    }
                    HoverHandler {
                        id: hover
                        cursorShape: Qt.PointingHandCursor
                    }
                    TapHandler {
                        gesturePolicy: TapHandler.ReleaseWithinBounds
                        onTapped: {
                            playerVm.requestSubtitleDownload(model.fileId);
                            root.close();
                        }
                    }

                    ColumnLayout {
                        id: rowCol
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.leftMargin: Theme.spacing
                        anchors.rightMargin: Theme.spacing
                        spacing: 2

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spacing
                            Label {
                                Layout.fillWidth: true
                                text: model.release && model.release.length > 0
                                    ? model.release : model.fileName
                                color: Theme.foreground
                                font.weight: Font.DemiBold
                                elide: Text.ElideRight
                            }
                            Label {
                                visible: model.moviehashMatch
                                text: "\uD83C\uDFAF"  // 🎯
                                color: Theme.foreground
                                ToolTip.visible: hovered
                                ToolTip.text: qsTr("Best match for this release")
                            }
                            Label {
                                visible: model.cached
                                text: "\u2B07" // ⬇
                                color: Theme.foregroundDim
                                ToolTip.visible: hovered
                                ToolTip.text: qsTr("Already cached on disk")
                            }
                            Label {
                                visible: model.active
                                text: "\u2713"
                                color: Theme.accent
                                ToolTip.visible: hovered
                                ToolTip.text: qsTr("Currently active in player")
                            }
                        }
                        Label {
                            Layout.fillWidth: true
                            text: (model.languageName || model.language) +
                                "  \u00B7  " + (model.format || "srt") +
                                "  \u00B7  " + qsTr("%1 downloads")
                                    .arg(model.downloadCount) +
                                (model.rating > 0
                                    ? "  \u00B7  \u2605 " + model.rating.toFixed(1)
                                    : "")
                            color: Theme.foregroundDim
                            font: Theme.smallFont
                            elide: Text.ElideRight
                        }
                    }
                }

                // Empty / loading state.
                Label {
                    anchors.centerIn: parent
                    visible: !playerVm.subtitleSearchActive
                        && (list.count === 0)
                        && playerVm.subtitleSearchError.length === 0
                    text: qsTr("No subtitles found.\nTry different filters.")
                    horizontalAlignment: Text.AlignHCenter
                    color: Theme.foregroundDim
                }
            }
        }
    }
}
