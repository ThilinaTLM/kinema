// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// One persistent download row. Built on `BaseListCard`, which owns
// the row chrome, hover / selection styling, padding, width, and
// right-click signalling. The chassis' hover-revealed
// `navigationHint` chevron carries this row's "tap to open detail"
// affordance.
//
// Body, in priority order:
//   1. Title block — primary line "Series · SxxEyy" or movie title;
//      secondary line is the episode title (hidden for movies).
//   2. Chip rail (uniform pill rhythm): State · Quality · Backend
//      with the live download rate in the trailing slot.
//   3. Stat strip with progress bar + "<percent> · <cached/total> ·
//      ETA <eta>" on a single caption line (active only).
//   4. Final size on a single caption line (completed only).
//   5. Attribution: "via <provider> · <release name>" plus
//      peers/seeds for active torrent rows.
//   6. Inline error message (failed only).
//
// Action rail (right side):
//   * Primary slot — `[▶ Play]` (Completed), `[↻ Retry]` (Failed),
//     `[▶ Resume]` icon-only (Paused), `[⏸]` icon-only (Active /
//     Idle / Resolving / Queued). Hidden for Cancelled. The slot
//     has a fixed width so the overflow column doesn't shift
//     between rows.
//   * Overflow button — same menu as the chassis' right-click
//     context menu.
BaseListCard {
    id: card

    required property string assetId
    required property string title
    required property string seriesTitle
    required property string episodeTitle
    required property string posterUrl
    required property int state
    required property string stateText
    required property string stateTone
    required property int mode
    required property string modeLabel
    required property bool canUpgrade
    required property bool canPause
    required property bool canResume
    required property bool hasPlayerAttached
    required property int backendKind
    required property string backendIcon
    required property string backendLabel
    required property bool pinned
    required property bool complete
    required property var progressFraction
    required property string progressText
    required property string sizeText
    required property string downloadRateText
    required property int peers
    required property int seeds
    required property string etaText
    required property string errorText
    required property string localDir
    required property string releaseName
    required property string qualityLabel
    required property string resolution
    required property string provider
    // Source-title identity for row-click navigation.
    required property string imdbId
    required property int kind
    required property int season
    required property int episode

    // Mirror domain::DownloadState integer values so the binding
    // code doesn't carry magic numbers.
    readonly property int stateQueued: 0
    readonly property int stateResolving: 1
    readonly property int stateActive: 2
    readonly property int stateIdle: 3
    readonly property int statePaused: 4
    readonly property int stateCompleted: 5
    readonly property int stateFailed: 6
    readonly property int stateCancelled: 7

    // domain::MediaKind: 0 = movie, 1 = series.
    readonly property int kindMovie: 0
    readonly property int kindSeries: 1

    readonly property real _progress: progressFraction !== undefined
        && progressFraction !== null
        ? progressFraction
        : -1

    readonly property string _primaryTitle: {
        if (card.kind === card.kindSeries
            && card.season >= 0 && card.episode >= 0) {
            const head = card.seriesTitle.length > 0
                ? card.seriesTitle
                : card.title;
            return i18nc("@title download row, series \u00b7 SxxEyy",
                "%1 \u00b7 S%2E%3",
                head, card._pad2(card.season),
                card._pad2(card.episode));
        }
        return card.title;
    }
    readonly property string _secondaryTitle: {
        if (card.kind === card.kindSeries
            && card.episodeTitle.length > 0) {
            return card.episodeTitle;
        }
        return "";
    }
    readonly property string _qualityChipText: card.qualityLabel.length > 0
        ? card.qualityLabel
        : card.resolution

    readonly property string _attributionText: {
        const parts = [];
        if (card.provider.length > 0) {
            parts.push(i18nc(
                "@label download provider, source aggregator",
                "via %1", card.provider));
        }
        if (card.releaseName.length > 0) {
            parts.push(card.releaseName);
        }
        if (card.backendKind === 0
            && (card.peers > 0 || card.seeds > 0)
            && !card.complete) {
            parts.push(i18ncp("@label torrent peers / seeds",
                "%1 peer", "%1 peers", card.peers));
            parts.push(i18ncp("@label torrent peers / seeds",
                "%1 seed", "%1 seeds", card.seeds));
        }
        return parts.join(" \u00b7 ");
    }

    function _pad2(n) {
        return n < 10 ? "0" + n : "" + n;
    }

    function _chipTone(tone) {
        switch (tone) {
        case "positive": return "positive";
        case "negative": return "negative";
        case "warn":     return "accent";
        default:         return "neutral";
        }
    }

    enabled: card.imdbId.length > 0 || card.assetId.length > 0

    // Hover-revealed chevron + tooltip via the chassis. Replaces the
    // inline QQC2.ToolTip the old DownloadRow carried.
    navigationHint: card.imdbId.length > 0
        ? i18nc("@info:tooltip open detail page for this title",
            "Open %1", card.title)
        : ""

    onClicked: {
        if (card.imdbId.length === 0) {
            return;
        }
        if (card.kind === card.kindSeries) {
            if (card.season >= 0 && card.episode >= 0) {
                shell.openSeriesDetailAt(card.imdbId,
                    card.title, card.season, card.episode);
            } else {
                shell.openSeriesDetail(card.imdbId, card.title);
            }
        } else {
            shell.openMovieDetail(card.imdbId, card.title);
        }
    }

    onContextMenuRequested: function (pt) {
        rowMenu.popup(pt.x, pt.y);
    }

    // Leading: 2:3 poster with optional pin overlay. Fills the
    // row's available height; width tracks the post-layout height
    // via the 2:3 aspect.
    leading: RowMediaThumbnail {
        Layout.fillHeight: true
        Layout.preferredHeight: Kirigami.Units.gridUnit * 4.5
        Layout.preferredWidth: Math.round(height / 1.5)
        url: card.posterUrl
        imageRole: "poster"
        fallbackIcon: "clapperboard"
        aspect: 1.5

        // Pin overlay. Anchored bottom-right inside the artwork
        // frame, same as before.
        Rectangle {
            visible: card.pinned
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.margins: Kirigami.Units.smallSpacing / 2
            width: Kirigami.Units.iconSizes.small
                + Kirigami.Units.smallSpacing
            height: width
            radius: width / 2
            color: Qt.alpha(Kirigami.Theme.backgroundColor, 0.78)
            border.color: Qt.alpha(Theme.accent, 0.55)
            border.width: 1

            Kirigami.Icon {
                anchors.centerIn: parent
                width: Kirigami.Units.iconSizes.small
                height: width
                source: AppIcons.url("pin", Theme.accent)
            }

            HoverHandler { id: pinHover }
            QQC2.ToolTip.text: i18nc(
                "@info:tooltip pinned download",
                "Kept \u2014 won't be evicted")
            QQC2.ToolTip.visible: pinHover.hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        }
    }

    // Body (default slot).
    QQC2.Label {
        Layout.fillWidth: true
        elide: Text.ElideRight
        text: card._primaryTitle
        font.weight: Font.DemiBold
        color: Theme.foreground
        onTruncatedChanged: card.titleTooltip
            = truncated ? text : ""
    }

    QQC2.Label {
        Layout.fillWidth: true
        visible: card._secondaryTitle.length > 0
        text: card._secondaryTitle
        wrapMode: Text.NoWrap
        elide: Text.ElideRight
        font.pointSize: Theme.captionFont.pointSize
        color: Theme.disabled
    }

    // Chip rail: State · Quality · Backend + live rate in trailing.
    RowChipRail {
        Layout.fillWidth: true
        chips: [
            {
                text: card.stateText,
                tone: card._chipTone(card.stateTone)
            },
            {
                text: card._qualityChipText,
                tone: "neutral"
            },
            {
                text: card.backendLabel,
                iconSource: card.backendIcon,
                iconColor: Theme.disabled,
                tone: "neutral"
            }
        ]

        QQC2.Label {
            visible: !card.complete
                && card.downloadRateText.length > 0
            text: card.downloadRateText
            color: Theme.foreground
            font.weight: Font.DemiBold
        }
    }

    // Progress bar + stat strip (active only).
    RowLayout {
        Layout.fillWidth: true
        spacing: Kirigami.Units.largeSpacing
        visible: card._progress >= 0 && !card.complete

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: Kirigami.Units.smallSpacing
            radius: height / 2
            color: Qt.alpha(Theme.foreground, 0.14)

            Rectangle {
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                width: parent.width
                    * Math.max(0, Math.min(1, card._progress))
                radius: parent.radius
                color: Theme.accent
            }
        }

        QQC2.Label {
            text: {
                const parts = [];
                if (card.progressText.length > 0)
                    parts.push(card.progressText);
                if (card.sizeText.length > 0)
                    parts.push(card.sizeText);
                if (card.etaText.length > 0)
                    parts.push(i18nc("@label download eta",
                        "ETA %1", card.etaText));
                return parts.join(" \u00b7 ");
            }
            visible: text.length > 0
            color: Theme.disabled
            font.pointSize: Theme.captionFont.pointSize
        }
    }

    QQC2.Label {
        Layout.fillWidth: true
        visible: card.complete && card.sizeText.length > 0
        text: card.sizeText
        color: Theme.disabled
        font.pointSize: Theme.captionFont.pointSize
    }

    QQC2.Label {
        Layout.fillWidth: true
        visible: card._attributionText.length > 0
        text: card._attributionText
        elide: Text.ElideRight
        font.pointSize: Theme.captionFont.pointSize
        color: Theme.disabled
    }

    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: card.errorText.length > 0
        type: Kirigami.MessageType.Error
        text: card.errorText
    }

    // Action row: contextual primary + overflow. The action row
    // lives below the body, so the buttons get their own line,
    // left-aligned with the rest of the right column.
    trailing: RowLayout {
        spacing: Kirigami.Units.smallSpacing

        QQC2.Button {
            id: primaryButton
            visible: card.state !== card.stateCancelled

            readonly property bool isFailed:
                card.state === card.stateFailed
            readonly property bool isPaused:
                card.state === card.statePaused
            readonly property bool isCompleted:
                card.state === card.stateCompleted

            flat: !isCompleted && !isFailed

            icon.source: AppIcons.url(isFailed
                ? "refresh-cw"
                : (isPaused || isCompleted ? "play" : "pause"))
            icon.color: AppIcons.controlColor(enabled, false)
            text: isFailed
                ? i18nc("@action:button retry a failed download",
                    "Retry")
                : (isCompleted
                    ? i18nc(
                        "@action:button play the cached file",
                        "Play")
                    : (isPaused
                        ? i18nc("@action:button", "Resume")
                        : i18nc("@action:button", "Pause")))
            display: QQC2.AbstractButton.TextBesideIcon
            onClicked: {
                if (isCompleted) {
                    downloadsVm.playDownload(card.assetId);
                } else if (isFailed) {
                    downloadsVm.retry(card.assetId);
                } else if (isPaused) {
                    downloadsVm.resumeDownload(card.assetId);
                } else {
                    downloadsVm.pauseDownload(card.assetId);
                }
            }
        }

        QQC2.ToolButton {
            icon.source: AppIcons.url("ellipsis-vertical")
            icon.color: AppIcons.controlColor(enabled, false)
            text: i18nc("@action:button overflow", "More")
            display: QQC2.AbstractButton.IconOnly
            QQC2.ToolTip.text: text
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            onClicked: rowMenu.popup()
        }
    }

    QQC2.Menu {
        id: rowMenu

        QQC2.MenuItem {
            text: i18nc("@action:inmenu play the cached file",
                "Play")
            icon.source: AppIcons.url("play")
            icon.color: AppIcons.controlColor(enabled, false)
            visible: card.complete
            height: visible ? implicitHeight : 0
            onTriggered: downloadsVm.playDownload(card.assetId)
        }
        QQC2.MenuItem {
            text: i18nc(
                "@action:inmenu open the local cache folder",
                "Open folder")
            icon.source: AppIcons.url("folder-open")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: card.localDir.length > 0
            onTriggered: downloadsVm.openLocalDir(card.assetId)
        }
        // "Keep file" covers two paths into Pinned: the OnDemand
        // → Full + Pinned upgrade (`canUpgrade` && !complete) and
        // the already-Full-but-ephemeral re-pin. Both end at the
        // same observable state, so we surface a single label.
        QQC2.MenuItem {
            text: i18nc("@action:inmenu mark this download as "
                + "user-requested so eviction skips it",
                "Keep file")
            icon.source: AppIcons.url("download")
            icon.color: AppIcons.controlColor(enabled, false)
            visible: !card.pinned
                && (card.canUpgrade || card.complete)
            height: visible ? implicitHeight : 0
            onTriggered: {
                if (card.canUpgrade && !card.complete) {
                    downloadsVm.upgradeToFull(card.assetId);
                } else {
                    downloadsVm.pin(card.assetId, true);
                }
            }
        }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu let cache eviction reclaim "
                + "this download when needed",
                "Allow eviction")
            icon.source: AppIcons.url("circle-dashed")
            icon.color: AppIcons.controlColor(enabled, false)
            visible: card.pinned
            height: visible ? implicitHeight : 0
            onTriggered: downloadsVm.pin(card.assetId, false)
        }
        QQC2.MenuSeparator { }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu", "Cancel")
            icon.source: AppIcons.url("x")
            icon.color: AppIcons.controlColor(enabled, false)
            enabled: card.state !== card.stateCompleted
                && card.state !== card.stateFailed
                && card.state !== card.stateCancelled
            onTriggered: downloadsVm.cancel(card.assetId)
        }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu",
                "Remove from list (keep files)")
            icon.source: AppIcons.url("list-x")
            icon.color: AppIcons.controlColor(enabled, false)
            onTriggered: downloadsVm.remove(card.assetId, false)
        }
        QQC2.MenuItem {
            text: i18nc("@action:inmenu",
                "Remove and delete files")
            icon.source: AppIcons.url("trash-2", AppIcons.negative)
            onTriggered: deleteConfirm.open()
        }
    }

    Kirigami.PromptDialog {
        id: deleteConfirm
        title: i18nc("@title:dialog", "Delete cached files?")
        subtitle: i18nc(
            "@info confirm before destructive remove",
            "Remove this download and delete its cached files? "
            + "This cannot be undone.")
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: i18nc("@action:button", "Cancel")
                onTriggered: deleteConfirm.close()
            },
            Kirigami.Action {
                text: i18nc("@action:button destructive", "Delete")
                icon.source: AppIcons.url("trash-2",
                    AppIcons.negative)
                onTriggered: {
                    downloadsVm.remove(card.assetId, true);
                    deleteConfirm.close();
                }
            }
        ]
    }
}
