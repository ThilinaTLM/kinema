// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// One persistent download row. Content-tile layout: a row reads as
// "a piece of watch-ready content, in some state", not as a transfer
// log entry.
//
// Body, in priority order:
//   1. Title block — primary line "Series · SxxEyy" or movie title;
//      secondary line is the episode title (hidden for movies).
//   2. Chip rail (uniform pill rhythm): State · Quality · Backend.
//      Mode chip removed — `stateText` already disambiguates Stream
//      vs Full ("Caching" / "Streaming" / "Downloading" /
//      "Downloading + Playing"); pinning surfaces as a small overlay
//      on the poster instead of as a chip.
//   3. Live transient on the chip rail: download rate (active rows).
//   4. Stat strip with progress bar + "<percent> · <cached/total> ·
//      ETA <eta>" on a single caption line (active only).
//   5. Final size on a single caption line (completed only).
//   6. Attribution: "via <provider> · <release name>" plus
//      peers/seeds for active torrent rows. Single elided line; full
//      release string is reachable via the row's tooltip.
//   7. Inline error message (failed only).
//
// Action rail (right side):
//   * Primary slot — `[▶ Play]` (Completed), `[↻ Retry]` (Failed),
//     `[▶ Resume]` icon-only (Paused), `[⏸]` icon-only (Active /
//     Idle / Resolving / Queued). Hidden for Cancelled. The slot has
//     a fixed width so the overflow column doesn't shift between
//     rows.
//   * Overflow menu.
//
// Row click target:
//   The whole card navigates to the source detail page, with a
//   tooltip on hover that names the destination. The chevron has been
//   removed because the inline `Play` / `Retry` is now the row's
//   strongest CTA.
//
// Overflow menu (top-down, useful → destructive):
//   - Play (completed only)
//   - Open folder
//   - Keep file (canUpgrade && !complete && !pinned, OR complete &&
//     !pinned)
//   - Allow eviction (pinned)
//   - ----
//   - Cancel (active states only)
//   - Remove from list (keep files)
//   - Remove and delete files (with PromptDialog confirmation)
QQC2.ItemDelegate {
    id: row

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

    // Mirror api::DownloadState integer values so the binding code
    // doesn't carry magic numbers.
    readonly property int stateQueued: 0
    readonly property int stateResolving: 1
    readonly property int stateActive: 2
    readonly property int stateIdle: 3
    readonly property int statePaused: 4
    readonly property int stateCompleted: 5
    readonly property int stateFailed: 6
    readonly property int stateCancelled: 7

    // api::MediaKind: 0 = movie, 1 = series.
    readonly property int kindMovie: 0
    readonly property int kindSeries: 1

    // Resolved progress in [0, 1], or -1 when unknown. The
    // `progressFraction` role can come through as `undefined` when
    // the role is null on the C++ side, so collapse both forms here.
    readonly property real _progress: progressFraction !== undefined
        && progressFraction !== null
        ? progressFraction
        : -1

    // ---- Derived display strings ------------------------------
    // Title line 1: "<series> · S02E22" for episodes, plain title
    // for movies (or for episodes without season/episode metadata).
    readonly property string _primaryTitle: {
        if (row.kind === row.kindSeries
            && row.season >= 0 && row.episode >= 0) {
            const head = row.seriesTitle.length > 0
                ? row.seriesTitle
                : row.title;
            return i18nc("@title download row, series \u00b7 SxxEyy",
                "%1 \u00b7 S%2E%3",
                head, row._pad2(row.season), row._pad2(row.episode));
        }
        return row.title;
    }
    // Title line 2: episode title for series rows, hidden for
    // movies. Empty when missing — the Label is hidden in that case.
    readonly property string _secondaryTitle: {
        if (row.kind === row.kindSeries
            && row.episodeTitle.length > 0) {
            return row.episodeTitle;
        }
        return "";
    }
    readonly property string _qualityChipText: row.qualityLabel.length > 0
        ? row.qualityLabel
        : row.resolution

    // Quiet attribution joins provider + release name + peers/seeds
    // (active torrents only) with " · ". Failed rows already surface
    // the full error in an InlineMessage, so the attribution stays
    // identical there too.
    readonly property string _attributionText: {
        const parts = [];
        if (row.provider.length > 0) {
            parts.push(i18nc("@label download provider, source aggregator",
                "via %1", row.provider));
        }
        if (row.releaseName.length > 0) {
            parts.push(row.releaseName);
        }
        if (row.backendKind === 0
            && (row.peers > 0 || row.seeds > 0)
            && !row.complete) {
            parts.push(i18ncp("@label torrent peers / seeds",
                "%1 peer", "%1 peers", row.peers));
            parts.push(i18ncp("@label torrent peers / seeds",
                "%1 seed", "%1 seeds", row.seeds));
        }
        return parts.join(" \u00b7 ");
    }

    function _pad2(n) {
        return n < 10 ? "0" + n : "" + n;
    }

    // Map api::DownloadItem stateTone to MetaChip tones. MetaChip
    // lacks a dedicated "warn" tone today, so warn falls through to
    // "accent" — visually close to Plasma's neutralTextColor tint.
    function _chipTone(tone) {
        switch (tone) {
        case "positive": return "positive";
        case "negative": return "negative";
        case "warn":     return "accent";
        default:         return "neutral";
        }
    }

    width: ListView.view ? ListView.view.width : implicitWidth
    padding: Theme.pageMargin
    implicitHeight: layout.implicitHeight + padding * 2
    hoverEnabled: true
    enabled: row.imdbId.length > 0 || row.assetId.length > 0

    // The row is the link to the source detail page. Tooltip surfaces
    // the destination on hover; the inline Play / Retry primary is
    // the strongest CTA.
    QQC2.ToolTip.text: row.imdbId.length > 0
        ? i18nc("@info:tooltip open detail page for this title",
            "Open %1", row.title)
        : ""
    QQC2.ToolTip.visible: row.hovered && row.imdbId.length > 0
    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay

    onClicked: {
        if (row.imdbId.length === 0) {
            return;
        }
        if (row.kind === row.kindSeries) {
            if (row.season >= 0 && row.episode >= 0) {
                mainController.openSeriesDetailAt(row.imdbId,
                    row.title, row.season, row.episode);
            } else {
                mainController.openSeriesDetail(row.imdbId, row.title);
            }
        } else {
            mainController.openMovieDetail(row.imdbId, row.title);
        }
    }

    background: Rectangle {
        radius: Kirigami.Units.cornerRadius
        color: Kirigami.Theme.alternateBackgroundColor
        border.color: Qt.alpha(Theme.foreground,
            row.hovered ? 0.18 : 0.08)
        border.width: 1

        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            color: row.hovered
                ? Qt.alpha(Theme.hover, 0.10)
                : "transparent"
        }
    }

    contentItem: RowLayout {
        id: layout
        spacing: Theme.groupSpacing

        // ---- Poster ---------------------------------------------
        Item {
            Layout.preferredWidth: Kirigami.Units.gridUnit * 3
            Layout.preferredHeight: Kirigami.Units.gridUnit * 4.5
            Layout.alignment: Qt.AlignTop

            Rectangle {
                anchors.fill: parent
                radius: Kirigami.Units.cornerRadius
                color: Kirigami.Theme.alternateBackgroundColor
                border.color: Qt.alpha(Theme.foreground, 0.10)
                border.width: 1
            }
            Image {
                id: poster
                anchors.fill: parent
                anchors.margins: 1
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                cache: true
                source: row.posterUrl.length > 0
                    ? "image://kinema/poster?u="
                        + encodeURIComponent(row.posterUrl)
                    : ""
                visible: status === Image.Ready
            }
            Kirigami.Icon {
                anchors.centerIn: parent
                width: Kirigami.Units.iconSizes.medium
                height: width
                source: AppIcons.url("clapperboard")
                color: Qt.alpha(Theme.foreground, 0.35)
                visible: poster.status !== Image.Ready
            }

            // Pin overlay. Communicates "this won't be evicted"
            // without spending a chip slot. Sits bottom-right so it
            // doesn't fight the poster art's typical focal point.
            Rectangle {
                visible: row.pinned
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
                QQC2.ToolTip.text: i18nc("@info:tooltip pinned download",
                    "Kept \u2014 won't be evicted")
                QQC2.ToolTip.visible: pinHover.hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
        }

        // ---- Body ----------------------------------------------
        ColumnLayout {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignTop
            spacing: Theme.inlineSpacing

            // Title line 1: "<series> · S02E22" (or movie title).
            QQC2.Label {
                Layout.fillWidth: true
                elide: Text.ElideRight
                text: row._primaryTitle
                font.weight: Font.DemiBold
                color: Theme.foreground
            }

            // Title line 2: episode title (series only).
            QQC2.Label {
                Layout.fillWidth: true
                visible: row._secondaryTitle.length > 0
                text: row._secondaryTitle
                wrapMode: Text.NoWrap
                elide: Text.ElideRight
                font.pointSize: Theme.captionFont.pointSize
                color: Theme.disabled
            }

            // ---- Chip rail: state · quality · backend · rate ---
            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.largeSpacing

                MetaChip {
                    text: row.stateText
                    tone: row._chipTone(row.stateTone)
                }

                MetaChip {
                    visible: row._qualityChipText.length > 0
                    text: row._qualityChipText
                    tone: "neutral"
                }

                MetaChip {
                    text: row.backendLabel
                    iconSource: row.backendIcon
                    iconColor: Theme.disabled
                    tone: "neutral"
                }

                Item { Layout.fillWidth: true }

                // Live download rate — eye-catching while active,
                // empty when paused / completed / failed.
                QQC2.Label {
                    visible: !row.complete
                        && row.downloadRateText.length > 0
                    text: row.downloadRateText
                    color: Theme.foreground
                    font.weight: Font.DemiBold
                }
            }

            // ---- Progress bar + consolidated stat strip ---------
            // Indeterminate state suppresses the bar (matches the
            // old `progressFraction !== undefined` gate).
            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.largeSpacing
                visible: row._progress >= 0 && !row.complete

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
                            * Math.max(0, Math.min(1, row._progress))
                        radius: parent.radius
                        color: Theme.accent
                    }
                }

                QQC2.Label {
                    text: {
                        const parts = [];
                        if (row.progressText.length > 0)
                            parts.push(row.progressText);
                        if (row.sizeText.length > 0)
                            parts.push(row.sizeText);
                        if (row.etaText.length > 0)
                            parts.push(i18nc("@label download eta",
                                "ETA %1", row.etaText));
                        return parts.join(" \u00b7 ");
                    }
                    visible: text.length > 0
                    color: Theme.disabled
                    font.pointSize: Theme.captionFont.pointSize
                }
            }

            // For completed rows, drop the bar and just surface the
            // final size so the layout stays informative without a
            // flat 100 % slug.
            QQC2.Label {
                Layout.fillWidth: true
                visible: row.complete && row.sizeText.length > 0
                text: row.sizeText
                color: Theme.disabled
                font.pointSize: Theme.captionFont.pointSize
            }

            // ---- Attribution: "via <provider> · <release name>"
            // plus peers/seeds on active torrent rows.
            QQC2.Label {
                Layout.fillWidth: true
                visible: row._attributionText.length > 0
                text: row._attributionText
                elide: Text.ElideRight
                font.pointSize: Theme.captionFont.pointSize
                color: Theme.disabled
            }

            // ---- Error message (failed only) --------------------
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: row.errorText.length > 0
                type: Kirigami.MessageType.Error
                text: row.errorText
            }
        }

        // ---- Action rail ----------------------------------------
        RowLayout {
            Layout.alignment: Qt.AlignTop
            spacing: Kirigami.Units.smallSpacing

            // Contextual primary slot. The wrapper Item keeps a
            // stable column width across rows so the overflow
            // button doesn't shift when the inner ToolButton hides
            // for Cancelled or swaps between IconOnly (transient)
            // and TextBesideIcon (high-stakes) labels.
            Item {
                Layout.preferredWidth: Kirigami.Units.gridUnit * 5
                Layout.preferredHeight: primaryButton.implicitHeight

                QQC2.Button {
                    id: primaryButton
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    visible: row.state !== row.stateCancelled

                    readonly property bool isFailed:
                        row.state === row.stateFailed
                    readonly property bool isPaused:
                        row.state === row.statePaused
                    readonly property bool isCompleted:
                        row.state === row.stateCompleted

                    flat: !isCompleted && !isFailed

                    icon.source: AppIcons.url(isFailed
                        ? "refresh-cw"
                        : (isPaused || isCompleted ? "play" : "pause"))
                    icon.color: AppIcons.controlColor(enabled, false)
                    text: isFailed
                        ? i18nc("@action:button retry a failed download",
                            "Retry")
                        : (isCompleted
                            ? i18nc("@action:button play the cached file",
                                "Play")
                            : (isPaused
                                ? i18nc("@action:button", "Resume")
                                : i18nc("@action:button", "Pause")))
                    display: (isCompleted || isFailed)
                        ? QQC2.AbstractButton.TextBesideIcon
                        : QQC2.AbstractButton.IconOnly
                    QQC2.ToolTip.text: text
                    QQC2.ToolTip.visible: hovered
                        && display === QQC2.AbstractButton.IconOnly
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    onClicked: {
                        if (isCompleted) {
                            downloadsVm.playDownload(row.assetId);
                        } else if (isFailed) {
                            downloadsVm.retry(row.assetId);
                        } else if (isPaused) {
                            downloadsVm.resumeDownload(row.assetId);
                        } else {
                            downloadsVm.pauseDownload(row.assetId);
                        }
                    }
                }
            }

            QQC2.ToolButton {
                Layout.alignment: Qt.AlignVCenter
                icon.source: AppIcons.url("ellipsis-vertical")
                icon.color: AppIcons.controlColor(enabled, false)
                text: i18nc("@action:button overflow", "More")
                display: QQC2.AbstractButton.IconOnly
                QQC2.ToolTip.text: text
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                onClicked: rowMenu.popup()

                QQC2.Menu {
                    id: rowMenu

                    QQC2.MenuItem {
                        text: i18nc("@action:inmenu play the cached file",
                            "Play")
                        icon.source: AppIcons.url("play")
                        icon.color: AppIcons.controlColor(enabled, false)
                        visible: row.complete
                        height: visible ? implicitHeight : 0
                        onTriggered: downloadsVm.playDownload(row.assetId)
                    }
                    QQC2.MenuItem {
                        text: i18nc("@action:inmenu open the local cache folder",
                            "Open folder")
                        icon.source: AppIcons.url("folder-open")
                        icon.color: AppIcons.controlColor(enabled, false)
                        enabled: row.localDir.length > 0
                        onTriggered: downloadsVm.openLocalDir(row.assetId)
                    }
                    // "Keep file" covers two paths into Pinned: the
                    // OnDemand -> Full + Pinned upgrade
                    // (`canUpgrade` && !complete) and the
                    // already-Full-but-ephemeral re-pin. Both end at
                    // the same observable state, so we surface a
                    // single label.
                    QQC2.MenuItem {
                        text: i18nc("@action:inmenu mark this download "
                            + "as user-requested so eviction skips it",
                            "Keep file")
                        icon.source: AppIcons.url("download")
                        icon.color: AppIcons.controlColor(enabled, false)
                        visible: !row.pinned
                            && (row.canUpgrade || row.complete)
                        height: visible ? implicitHeight : 0
                        onTriggered: {
                            if (row.canUpgrade && !row.complete) {
                                downloadsVm.upgradeToFull(row.assetId);
                            } else {
                                downloadsVm.pin(row.assetId, true);
                            }
                        }
                    }
                    QQC2.MenuItem {
                        text: i18nc("@action:inmenu let cache eviction "
                            + "reclaim this download when needed",
                            "Allow eviction")
                        icon.source: AppIcons.url("circle-dashed")
                        icon.color: AppIcons.controlColor(enabled, false)
                        visible: row.pinned
                        height: visible ? implicitHeight : 0
                        onTriggered: downloadsVm.pin(row.assetId, false)
                    }
                    QQC2.MenuSeparator { }
                    QQC2.MenuItem {
                        text: i18nc("@action:inmenu", "Cancel")
                        icon.source: AppIcons.url("x")
                        icon.color: AppIcons.controlColor(enabled, false)
                        enabled: row.state !== row.stateCompleted
                            && row.state !== row.stateFailed
                            && row.state !== row.stateCancelled
                        onTriggered: downloadsVm.cancel(row.assetId)
                    }
                    QQC2.MenuItem {
                        text: i18nc("@action:inmenu",
                            "Remove from list (keep files)")
                        icon.source: AppIcons.url("list-x")
                        icon.color: AppIcons.controlColor(enabled, false)
                        onTriggered: downloadsVm.remove(row.assetId, false)
                    }
                    QQC2.MenuItem {
                        text: i18nc("@action:inmenu",
                            "Remove and delete files")
                        icon.source: AppIcons.url("trash-2",
                            AppIcons.negative)
                        onTriggered: deleteConfirm.open()
                    }
                }

                Kirigami.PromptDialog {
                    id: deleteConfirm
                    title: i18nc("@title:dialog", "Delete cached files?")
                    subtitle: i18nc("@info confirm before destructive remove",
                        "Remove this download and delete its cached files? "
                        + "This cannot be undone.")
                    standardButtons: Kirigami.Dialog.NoButton
                    customFooterActions: [
                        Kirigami.Action {
                            text: i18nc("@action:button", "Cancel")
                            onTriggered: deleteConfirm.close()
                        },
                        Kirigami.Action {
                            text: i18nc("@action:button destructive",
                                "Delete")
                            icon.source: AppIcons.url("trash-2",
                                AppIcons.negative)
                            onTriggered: {
                                downloadsVm.remove(row.assetId, true);
                                deleteConfirm.close();
                            }
                        }
                    ]
                }
            }
        }
    }
}
