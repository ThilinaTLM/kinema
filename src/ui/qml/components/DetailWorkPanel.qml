// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Right-hand work surface for detail pages: streams and subtitles are adjacent
// tabs, sharing the same playback context prepared by the detail VM.
Item {
    id: panel

    property var detailVm
    property var subtitleVm: subtitlesVm
    property bool subtitlesAvailable: true
    property string subtitlesUnavailableText: i18nc("@info", "Pick an episode first.")

    signal subtitlesTabRequested()

    property bool switchingInternally: false

    function showStreams() {
        switchingInternally = true;
        tabs.currentIndex = 0;
        switchingInternally = false;
    }

    function showSubtitles() {
        unavailableMessage.visible = false;
        switchingInternally = true;
        tabs.currentIndex = 1;
        switchingInternally = false;
    }

    function requestSubtitlesTab() {
        if (!subtitlesAvailable) {
            unavailableMessage.text = subtitlesUnavailableText;
            unavailableMessage.visible = true;
            showStreams();
            return;
        }
        unavailableMessage.visible = false;
        subtitlesTabRequested();
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Kirigami.Units.largeSpacing
        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            QQC2.TabBar {
                id: tabs
                Layout.fillWidth: true
                onCurrentIndexChanged: {
                    if (currentIndex === 1 && !panel.switchingInternally) {
                        panel.requestSubtitlesTab();
                    }
                }

                QQC2.TabButton {
                    text: panel.detailVm && panel.detailVm.streams
                        && panel.detailVm.streams.count > 0
                        ? i18ncp("@title:tab", "%1 stream", "%1 streams",
                            panel.detailVm.streams.count)
                        : i18nc("@title:tab", "Streams")
                }
                QQC2.TabButton {
                    text: i18nc("@title:tab", "Subtitles")
                }
            }

            QQC2.ToolButton {
                icon.name: "view-sort-ascending"
                text: i18nc("@action:button", "Sort streams")
                display: QQC2.AbstractButton.IconOnly
                enabled: panel.detailVm && panel.detailVm.streams
                    && panel.detailVm.streams.count > 0
                onClicked: sortMenu.popup()
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                QQC2.ToolTip.text: text
            }
        }

        Kirigami.InlineMessage {
            id: unavailableMessage
            Layout.fillWidth: true
            type: Kirigami.MessageType.Information
            visible: false
            showCloseButton: true
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: tabs.currentIndex

            StreamsList {
                Layout.fillWidth: true
                Layout.fillHeight: true
                vm: panel.detailVm
            }

            SubtitlesPanel {
                Layout.fillWidth: true
                Layout.fillHeight: true
                vm: panel.subtitleVm
                compact: true
                showContextTitle: true
                showCancelButton: false
            }
        }
    }

    StreamSortMenu {
        id: sortMenu
        vm: panel.detailVm
    }
}
