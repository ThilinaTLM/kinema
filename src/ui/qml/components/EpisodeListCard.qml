// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// One episode row: 16:9 still · "N · Title" · release date / upcoming
// pill, with a one-line description below. Tapping the row pushes
// `StreamsPage`. Built on `BaseListCard`, which owns the row chrome,
// hover / selection styling, padding, width, right-click signalling,
// and the inline progress bar.
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
        Layout.preferredHeight: Kirigami.Units.gridUnit * 4
        Layout.preferredWidth: Math.round(height * 16 / 9)
        url: card.thumbnailUrl
        imageRole: "still"
        fallbackIcon: "play"
        aspect: 9 / 16
    }

    // Body (default slot): title row + description. Inline progress
    // bar is removed — chassis renders it from `card.progress` when
    // !watched (gated below by clearing progress in that case).
    RowLayout {
        Layout.fillWidth: true
        spacing: Theme.inlineSpacing

        QQC2.Label {
            id: titleLabel
            text: i18nc("@info episode title prefix",
                "%1 \u00b7 %2",
                card.episodeNumber, card.episodeTitle)
            elide: Text.ElideRight
            Layout.fillWidth: true
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
    }

    QQC2.Label {
        Layout.fillWidth: true
        visible: card.description.length > 0
        text: card.description
        wrapMode: Text.WordWrap
        maximumLineCount: 2
        elide: Text.ElideRight
        font.pointSize: Theme.captionFont.pointSize
        color: Theme.disabled
    }

    // Action row: Mark-Watched toggle. The full label lives in the
    // right-click menu as well; the dedicated action row affords
    // the button a permanent home so the affordance no longer
    // needs to hide behind hover.
    trailing: QQC2.ToolButton {
        visible: !card.isUpcoming
        icon.source: AppIcons.url(
            card.watched ? "circle-dashed" : "circle-check")
        icon.color: card.watched
            ? Theme.positive
            : Theme.foreground
        text: card.watched
            ? i18nc("@action:button", "Mark Unwatched")
            : i18nc("@action:button", "Mark Watched")
        display: QQC2.AbstractButton.TextBesideIcon
        onClicked: card.toggleWatchedRequested()
    }

    QQC2.Menu {
        id: rowMenu

        QQC2.MenuItem {
            text: i18nc("@action:inmenu open streams for this episode",
                "Find streams\u2026")
            icon.source: AppIcons.url("play")
            icon.color: AppIcons.foreground
            onTriggered: card.clicked()
        }
        QQC2.MenuItem {
            visible: !card.isUpcoming
            height: visible ? implicitHeight : 0
            text: card.watched
                ? i18nc("@action:inmenu", "Mark Unwatched")
                : i18nc("@action:inmenu", "Mark Watched")
            icon.source: AppIcons.url(
                card.watched ? "circle-dashed" : "circle-check")
            icon.color: card.watched
                ? AppIcons.positive
                : AppIcons.foreground
            onTriggered: card.toggleWatchedRequested()
        }
    }
}
