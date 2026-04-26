// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import dev.tlmtech.kinema.player

import "../components"

/**
 * "Player info" overlay reachable from the top-bar info button or
 * the `?` shortcut. Two sections today:
 *
 *   - About this stream — title chips at the top, a 2-column
 *     resolution / codec / channels / source grid, populated from
 *     `playerVm.streamInfo`.
 *   - Keyboard shortcuts — sectioned (`Playback`, `Navigation`, …)
 *     `playerVm.shortcutSections` rendered as a Repeater of
 *     SectionHeader + 2-column rows.
 *
 * Designed to grow: drop another `Section { … }` into the column
 * (e.g. "Subtitles", "Cache stats") without touching this file's
 * structure.
 */
Item {
    id: root
    anchors.fill: parent
    visible: opacity > 0.001
    opacity: playerVm.infoOverlayVisible ? 1.0 : 0.0
    Behavior on opacity { NumberAnimation { duration: Theme.fadeMs } }

    // Below the compact breakpoint the stream-info grid and the
    // per-shortcut-row grid collapse to one column so labels stack
    // above their values / actions instead of clipping.
    readonly property bool isCompact: width < Theme.compactBreakpoint

    // Backdrop dim — clicking outside the panel closes the overlay.
    Rectangle {
        anchors.fill: parent
        color: Theme.overlayShade
        TapHandler {
            onTapped: playerVm.setInfoOverlayVisible(false)
        }
    }

    // Esc closes too.
    Keys.onEscapePressed: event => {
        playerVm.setInfoOverlayVisible(false);
        event.accepted = true;
    }
    focus: visible

    PopupPanel {
        anchors.centerIn: parent
        // Cap at ~40 grid units, never wider than the scene less the
        // standard outer breathing room (3× large spacing each side).
        width: Math.min(Theme.gridUnit * 40,
            root.width - Theme.spacingLg * 6)
        height: Math.min(Theme.gridUnit * 38,
            root.height - Theme.spacingLg * 6)
        title: qsTr("Player info")
        onCloseRequested: playerVm.setInfoOverlayVisible(false)

        Flickable {
            anchors.fill: parent
            contentWidth: width
            contentHeight: column.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds

            ColumnLayout {
                id: column
                width: parent.width
                spacing: Theme.spacingLg

                // ---- About this stream ---------------------------
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spacing

                    SectionHeader {
                        Layout.fillWidth: true
                        text: qsTr("About this stream")
                    }

                    Label {
                        Layout.fillWidth: true
                        Layout.leftMargin: Theme.spacingXs
                        text: playerVm.mediaTitle
                        color: Theme.foreground
                        font: Theme.titleFont
                        elide: Text.ElideRight
                        visible: text.length > 0
                    }
                    Label {
                        Layout.fillWidth: true
                        Layout.leftMargin: Theme.spacingXs
                        text: playerVm.mediaSubtitle
                        color: Theme.foregroundDim
                        elide: Text.ElideRight
                        visible: text.length > 0
                    }
                    MediaChips {
                        Layout.fillWidth: true
                        Layout.leftMargin: Theme.spacingXs
                        chips: playerVm.mediaChips
                        visible: chips.length > 0
                    }

                    GridLayout {
                        id: streamGrid
                        Layout.fillWidth: true
                        Layout.leftMargin: Theme.spacingXs
                        Layout.topMargin: Theme.spacing
                        columns: root.isCompact ? 1 : 2
                        rowSpacing: Theme.spacingXs
                        columnSpacing: Theme.spacingLg

                        function _fmtRes(info) {
                            if (!info || !info.width || !info.height)
                                return qsTr("Unknown");
                            let s = info.width + " × " + info.height;
                            if (info.fps && info.fps > 0)
                                s += "  •  " + info.fps.toFixed(2)
                                  + " " + qsTr("fps");
                            return s;
                        }
                        function _fmtCodec(name) {
                            return (name && name.length > 0)
                                ? name.toUpperCase()
                                : qsTr("Unknown");
                        }
                        function _fmtAudio(info) {
                            let s = _fmtCodec(info.audioCodec);
                            if (info.audioChannels && info.audioChannels > 0)
                                s += "  •  " + info.audioChannels
                                  + " " + qsTr("ch");
                            return s;
                        }
                        function _fmtTracks(info) {
                            const a = info.audioTrackCount || 0;
                            const s = info.subtitleTrackCount || 0;
                            return qsTr("%1 audio, %2 subtitle")
                                .arg(a).arg(s);
                        }

                        Label {
                            text: qsTr("Resolution")
                            color: Theme.foregroundDim
                        }
                        Label {
                            Layout.fillWidth: true
                            text: streamGrid._fmtRes(playerVm.streamInfo)
                            color: Theme.foreground
                            font: Theme.monoFont
                        }

                        Label {
                            text: qsTr("Video codec")
                            color: Theme.foregroundDim
                        }
                        Label {
                            Layout.fillWidth: true
                            text: streamGrid._fmtCodec(
                                playerVm.streamInfo.videoCodec)
                            color: Theme.foreground
                            font: Theme.monoFont
                        }

                        Label {
                            text: qsTr("Audio")
                            color: Theme.foregroundDim
                        }
                        Label {
                            Layout.fillWidth: true
                            text: streamGrid._fmtAudio(playerVm.streamInfo)
                            color: Theme.foreground
                            font: Theme.monoFont
                        }

                        Label {
                            text: qsTr("Container")
                            color: Theme.foregroundDim
                            visible: (playerVm.streamInfo.container || "").length > 0
                        }
                        Label {
                            Layout.fillWidth: true
                            text: playerVm.streamInfo.container || ""
                            color: Theme.foreground
                            font: Theme.monoFont
                            visible: text.length > 0
                        }

                        Label {
                            text: qsTr("HDR")
                            color: Theme.foregroundDim
                            visible: playerVm.streamInfo.hdr === true
                        }
                        Label {
                            Layout.fillWidth: true
                            text: qsTr("Yes")
                            color: Theme.accent
                            font.weight: Font.DemiBold
                            visible: playerVm.streamInfo.hdr === true
                        }

                        Label {
                            text: qsTr("Tracks")
                            color: Theme.foregroundDim
                        }
                        Label {
                            Layout.fillWidth: true
                            text: streamGrid._fmtTracks(playerVm.streamInfo)
                            color: Theme.foreground
                        }

                        Label {
                            text: qsTr("Source")
                            color: Theme.foregroundDim
                            Layout.alignment: Qt.AlignTop
                            visible: (playerVm.streamInfo.sourceUrl || "").length > 0
                        }
                        Label {
                            Layout.fillWidth: true
                            text: playerVm.streamInfo.sourceUrl || ""
                            color: Theme.foreground
                            font: Theme.monoSmallFont
                            elide: Text.ElideMiddle
                            wrapMode: Text.NoWrap
                            visible: text.length > 0
                        }
                    }
                }

                // ---- Keyboard shortcuts -------------------------
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spacingLg

                    Repeater {
                        model: playerVm.shortcutSections
                        delegate: ColumnLayout {
                            id: sectionDelegate
                            required property var modelData
                            Layout.fillWidth: true
                            spacing: Theme.spacingXs

                            SectionHeader {
                                Layout.fillWidth: true
                                text: sectionDelegate.modelData.title
                            }

                            Repeater {
                                model: sectionDelegate.modelData.rows
                                // GridLayout (not RowLayout) so the
                                // column count flips with isCompact:
                                // 2-col on wide screens, stacked
                                // (keys above action) when compact.
                                delegate: GridLayout {
                                    id: rowDelegate
                                    required property var modelData
                                    Layout.fillWidth: true
                                    Layout.leftMargin: Theme.spacingXs
                                    columns: root.isCompact ? 1 : 2
                                    rowSpacing: 0
                                    columnSpacing: Theme.spacing
                                    Label {
                                        Layout.preferredWidth:
                                            root.isCompact
                                                ? -1
                                                : Theme.gridUnit * 14
                                        Layout.fillWidth: root.isCompact
                                        text: rowDelegate.modelData.keys
                                        color: Theme.accent
                                        font: Theme.monoFont
                                    }
                                    Label {
                                        Layout.fillWidth: true
                                        text: rowDelegate.modelData.action
                                        color: Theme.foreground
                                        wrapMode: Text.WordWrap
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
