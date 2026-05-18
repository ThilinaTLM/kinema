// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// One episode row. Fixed three-row body so every episode delegate
// has a consistent row height:
//
//   1. Title line   — "N · Title", watched indicator, release date /
//                     Upcoming pill. All left-aligned; a trailing
//                     flex spacer absorbs leftover width so nothing
//                     gets pushed to the row's right edge.
//   2. Description  — always reserves exactly two lines via
//                     `FontMetrics`, with `WordWrap` + `ElideRight`
//                     so longer descriptions truncate cleanly and
//                     shorter ones (or empty ones, e.g. upcoming
//                     episodes) still take the same vertical space.
//   3. Action row   — regular `QQC2.Button` (not flat) for the
//                     Mark Watched / Mark Unwatched toggle, given a
//                     permanent home in the dedicated action row.
//
// Tapping the row pushes `StreamsPage`. Built on `BaseListCard`,
// which owns the row chrome, hover / selection styling, padding,
// width, right-click signalling, and the inline progress bar.
BaseListCard {
    id: card

    property int episodeNumber: 0
    property string episodeTitle
    property string description
    property string releasedText
    property bool isUpcoming: false
    property string thumbnailUrl
    property bool watched: false

    // `selected` and `progress` come from BaseListCard.

    signal toggleWatchedRequested()

    onContextMenuRequested: function (pt) {
        rowMenu.popup(pt.x, pt.y);
    }
    // Activation = same affordance as tap (open StreamsPage).
    onActivated: card.clicked()

    Keys.onPressed: function (event) {
        if (event.key === Qt.Key_M && !card.isUpcoming) {
            card.toggleWatchedRequested();
            event.accepted = true;
        }
    }

    // 16:9 episode still. Fills the row's available height; width
    // tracks the post-layout height via the 16:9 aspect.
    leading: RowMediaThumbnail {
        Layout.fillHeight: true
        Layout.preferredWidth: Math.round(height * 16 / 9)
        url: card.thumbnailUrl
        imageRole: "still"
        fallbackIcon: "play"
        aspect: 9 / 16
    }

    // FontMetrics for the description label — used to reserve
    // exactly two lines of vertical space regardless of how many
    // lines the actual text occupies. Keeps every episode row at
    // the same height.
    FontMetrics {
        id: descriptionMetrics
        font.pointSize: Theme.captionFont.pointSize
    }

    // Line 1: title + watched indicator + release / upcoming pill.
    // Title sizes to its content with a soft cap and elide so long
    // titles don't drag everything to the right edge; the trailing
    // flex spacer absorbs leftover width so the rest of the line
    // stays packed against the title on the left.
    RowLayout {
        Layout.fillWidth: true
        spacing: Theme.inlineSpacing

        QQC2.Label {
            id: titleLabel
            text: i18nc("@info episode title prefix",
                "%1 \u00b7 %2",
                card.episodeNumber, card.episodeTitle)
            elide: Text.ElideRight
            Layout.maximumWidth: Kirigami.Units.gridUnit * 30
            font.pointSize: Theme.defaultFont.pointSize
            font.weight: Font.DemiBold
            color: Theme.foreground
            onTruncatedChanged: card.titleTooltip
                = truncated ? text : ""
        }
        Kirigami.Icon {
            visible: card.watched
            Layout.preferredWidth: Kirigami.Units.iconSizes.small
            Layout.preferredHeight: width
            source: AppIcons.url("circle-check")
            color: Theme.positive
        }
        UpcomingBadge {
            visible: card.isUpcoming
            releaseDateText: card.releasedText
        }
        QQC2.Label {
            visible: !card.isUpcoming
                && card.releasedText.length > 0
            text: card.releasedText
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
        }
        Item { Layout.fillWidth: true }
    }

    // Line 2: description, always reserves exactly two lines of
    // vertical space — `WordWrap` + `maximumLineCount: 2` give
    // long descriptions an ellipsis, and an explicit
    // `Layout.preferredHeight` keeps short or empty descriptions
    // at the same height as the multi-line case.
    QQC2.Label {
        Layout.fillWidth: true
        Layout.preferredHeight: descriptionMetrics.height * 2
        verticalAlignment: Text.AlignTop
        text: card.description
        wrapMode: Text.WordWrap
        maximumLineCount: 2
        elide: Text.ElideRight
        font.pointSize: Theme.captionFont.pointSize
        color: Theme.disabled
    }

    // Line 3: action row — regular Button (not flat ToolButton) so
    // the affordance reads as a permanent, primary control.
    trailing: QQC2.Button {
        visible: !card.isUpcoming
        icon.source: AppIcons.url(
            card.watched ? "circle-dashed" : "circle-check",
            AppIcons.controlColor(enabled, highlighted))
        icon.color: AppIcons.controlColor(enabled, highlighted)
        text: card.watched
            ? i18nc("@action:button", "Mark Unwatched")
            : i18nc("@action:button", "Mark Watched")
        display: QQC2.AbstractButton.TextBesideIcon
        onClicked: card.toggleWatchedRequested()
    }

    KinemaMenu {
        id: rowMenu

        KinemaMenuItem {
            iconName: "list-video"
            label: i18nc("@action:inmenu episode row", "Streams")
            // Upcoming episodes have no streams yet; the indexers
            // return nothing useful for unaired airdates.
            enabled: !card.isUpcoming
            onTriggered: card.clicked()
        }
        QQC2.MenuSeparator {
            visible: !card.isUpcoming
        }
        KinemaMenuItem {
            iconName: card.watched ? "circle-dashed" : "circle-check"
            label: card.watched
                ? i18nc("@action:inmenu episode row", "Mark Unwatched")
                : i18nc("@action:inmenu episode row", "Mark Watched")
            visible: !card.isUpcoming
            onTriggered: card.toggleWatchedRequested()
        }
        QQC2.MenuSeparator { }
        KinemaMenuItem {
            iconName: "copy"
            label: i18nc("@action:inmenu episode row", "Copy Title")
            enabled: card.episodeTitle.length > 0
            onTriggered: shell.copyToClipboard(card.episodeTitle,
                i18nc("@info:status",
                    "Episode title copied to clipboard"))
        }
    }
}
