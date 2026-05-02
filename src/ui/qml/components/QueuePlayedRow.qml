// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// One de-emphasized row for an item that has finished playing in
// this session. Sits above the active row in the queue page so the
// user can replay finished items via `Previous` / a tap, but it is
// visually muted so the eye is drawn to the lead row and the
// pending range below.
//
// Layout mirrors `QueueCompactRow` (poster on the left, full row
// height, meta on the right) so the page rhythm is consistent;
// only the styling differs. No drag handle, no quality chips, no
// row-trailing actions \u2014 everything funnels through the overflow
// menu.
QQC2.ItemDelegate {
    id: row

    property int rowIndex: -1
    property string title: ""
    property string subtitle: ""
    property string posterUrl: ""

    signal replayRequested()
    signal removeRequested()
    signal overflowRequested()

    width: ListView.view
        ? ListView.view.width - ListView.view.leftMargin
            - ListView.view.rightMargin
        : implicitWidth

    // Compact vertical footprint (~60% of a compact row) so the
    // played stack doesn't dominate the page.
    readonly property real posterHeight:
        Math.round(Kirigami.Units.gridUnit * 3.5)
    readonly property real posterWidth:
        Math.round(posterHeight / 1.5)

    padding: Math.round(Theme.inlineSpacing * 1.25)
    implicitHeight: Math.max(posterHeight, layout.implicitHeight)
        + padding * 2

    onClicked: row.replayRequested()

    background: Rectangle {
        radius: Kirigami.Units.cornerRadius
        color: row.hovered
            ? Qt.alpha(Kirigami.Theme.alternateBackgroundColor, 0.55)
            : "transparent"
        border.color: Qt.alpha(Theme.foreground,
            row.hovered ? 0.10 : 0.06)
        border.width: 1
    }

    contentItem: RowLayout {
        id: layout
        spacing: Theme.groupSpacing

        // Poster
        Item {
            Layout.alignment: Qt.AlignVCenter
            Layout.preferredWidth: row.posterWidth
            Layout.preferredHeight: row.posterHeight

            Rectangle {
                anchors.fill: parent
                radius: Kirigami.Units.cornerRadius
                color: Qt.alpha(Theme.foreground, 0.06)
                border.color: Qt.alpha(Theme.foreground, 0.08)
                border.width: 1
            }
            Image {
                anchors.fill: parent
                anchors.margins: 1
                source: row.posterUrl.length > 0
                    ? "image://kinema/queue?u=" + row.posterUrl
                    : ""
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                cache: true
                visible: status === Image.Ready
                opacity: 0.55
            }
            Kirigami.Icon {
                anchors.centerIn: parent
                width: Kirigami.Units.iconSizes.small
                height: width
                source: AppIcons.url("history")
                color: Qt.alpha(Theme.foreground, 0.45)
            }

            // Soft scrim so the poster feels "consumed" without
            // hiding it. Mirrors the muted text colour.
            Rectangle {
                anchors.fill: parent
                radius: Kirigami.Units.cornerRadius
                color: Qt.alpha(Theme.background, 0.30)
            }
        }

        // Meta column
        ColumnLayout {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignVCenter
            spacing: Math.round(Theme.inlineSpacing / 2)

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.inlineSpacing

                MetaChip {
                    text: i18nc("@info queue badge for finished item",
                        "Played")
                    tone: "neutral"
                }

                QQC2.Label {
                    Layout.fillWidth: true
                    text: row.title
                    color: Theme.disabled
                    font.weight: Font.Medium
                    font.strikeout: true
                    elide: Text.ElideRight
                    maximumLineCount: 1
                    wrapMode: Text.NoWrap
                }
            }

            QQC2.Label {
                Layout.fillWidth: true
                visible: row.subtitle.length > 0
                text: row.subtitle
                color: Theme.disabled
                font.pointSize: Theme.captionFont.pointSize
                elide: Text.ElideRight
                maximumLineCount: 1
                wrapMode: Text.NoWrap
            }
        }

        // Trailing replay glyph (tap target is the row itself; the
        // glyph just signals the affordance).
        Kirigami.Icon {
            Layout.alignment: Qt.AlignVCenter
            Layout.preferredWidth: Kirigami.Units.iconSizes.small
            Layout.preferredHeight: width
            source: AppIcons.url("circle-play")
            color: row.hovered
                ? Theme.accent
                : Qt.alpha(Theme.foreground, 0.40)
        }

        QQC2.ToolButton {
            Layout.alignment: Qt.AlignVCenter
            icon.source: AppIcons.url("ellipsis")
            icon.color: AppIcons.controlColor(enabled, false)
            display: QQC2.AbstractButton.IconOnly
            text: i18nc("@action:button row actions", "More actions")
            onClicked: row.overflowRequested()
            QQC2.ToolTip.text: text
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        }
    }
}
