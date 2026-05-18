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
// affordance. The chassis also paints the row's progress bar
// (`card.progress`) between the body and the trailing action row
// so the body never has to host the bar itself.
//
// Body is a fixed 3-line column — the same shape in every state,
// so the row's height doesn't jitter as a download transitions
// between Queued → Active → Completed → Failed:
//
//   1. Title       — `Series · SxxEyy · Episode Title` for series,
//                    plain movie title otherwise. Left-aligned,
//                    elides on narrow rows; hover tooltip surfaces
//                    the full string.
//   2. Meta row    — `State · Quality · <icon> Backend · status
//                    · rate`. Dot-separated caption tokens on a
//                    single caption-sized line, so the row's
//                    height stays identical whether or not the
//                    optional tokens (status, rate) are present.
//                    The State token recolours by
//                    `card.stateTone` (positive/negative/accent),
//                    Backend renders an inline `Kirigami.Icon` +
//                    label pair, the status caption recolours
//                    `Theme.negative` on failed rows, and the
//                    rate keeps `Theme.foreground` / DemiBold so
//                    it scans as the live metric on the line.
//   3. Attribution — `via <provider> · <release name>` plus
//                    peers/seeds for active torrent rows; falls
//                    back to `Unknown source` so the body keeps
//                    its fixed line count.
//
// Action rail (trailing slot, rendered as its own line below the
// body by the chassis):
//   * Primary slot — `[▶ Play]` (Completed), `[↻ Retry]` (Failed),
//     `[▶ Resume]` (Paused), `[⏸ Pause]` (Active / Idle /
//     Resolving / Queued). Hidden for Cancelled.
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

    // domain::DownloadMode integer values.
    readonly property int modeOnDemand: 0
    readonly property int modeFull: 1

    // domain::MediaKind: 0 = movie, 1 = series.
    readonly property int kindMovie: 0
    readonly property int kindSeries: 1

    // ---- Primary-action kind ------------------------------------
    // Derived once from (state, mode, hasPlayerAttached, canUpgrade)
    // and consumed by the button below. Centralising the decision
    // here keeps the destructive-while-playing guards in one place
    // and prevents "Pause" from being offered when pausing would
    // starve the embedded player attached to this row.
    readonly property int primaryNone:    0
    readonly property int primaryPause:   1
    readonly property int primaryResume:  2
    readonly property int primaryPlay:    3
    readonly property int primaryRetry:   4
    readonly property int primaryKeep:    5

    readonly property int _primaryKind: {
        switch (card.state) {
        case card.stateCancelled:
        case card.stateResolving:
            return card.primaryNone;
        case card.stateFailed:
            return card.primaryRetry;
        case card.statePaused:
            return card.primaryResume;
        case card.stateCompleted:
            return card.primaryPlay;
        case card.stateIdle:
            return card.canUpgrade
                ? card.primaryKeep
                : card.primaryNone;
        case card.stateQueued:
            return card.primaryPause;
        case card.stateActive:
            // Pausing while a player is consuming the asset
            // (OnDemand stream, or Full-with-player) would break
            // playback. Surface "Play" instead — it re-invokes the
            // play pipeline, which short-circuits to the existing
            // session / cache. The Full+player case can still pause
            // via the overflow menu, with a confirm dialog.
            return card.hasPlayerAttached
                ? card.primaryPlay
                : card.primaryPause;
        }
        return card.primaryNone;
    }

    readonly property real _progress: progressFraction !== undefined
        && progressFraction !== null
        ? progressFraction
        : -1

    // Headline — single bold line. For series we fold the episode
    // title into the headline (`Series · S04E07 · Something
    // Unforgivable`) so the body keeps its fixed line count; long
    // strings elide and the chassis' truncation tooltip surfaces
    // the full text on hover.
    readonly property string _headlineText: {
        if (card.kind === card.kindSeries
            && card.season >= 0 && card.episode >= 0) {
            const head = card.seriesTitle.length > 0
                ? card.seriesTitle
                : card.title;
            const base = i18nc("@title download row, series \u00b7 SxxEyy",
                "%1 \u00b7 S%2E%3",
                head, card._pad2(card.season),
                card._pad2(card.episode));
            if (card.episodeTitle.length > 0) {
                return i18nc(
                    "@title download row, series-ep \u00b7 episode title",
                    "%1 \u00b7 %2", base, card.episodeTitle);
            }
            return base;
        }
        return card.title;
    }

    readonly property string _qualityChipText: card.qualityLabel.length > 0
        ? card.qualityLabel
        : card.resolution

    // Status caption — one line, content depends on state. Failed
    // rows are coloured `Theme.negative` and prefixed with `⚠` so
    // the line acts as the row's error surface (replaces the
    // separate Kirigami.InlineMessage the body used to host).
    readonly property string _statusText: {
        switch (card.state) {
        case card.stateQueued:
            return i18nc("@label download status",
                "Waiting in queue");
        case card.stateResolving:
            return i18nc("@label download status",
                "Resolving stream sources\u2026");
        case card.stateActive: {
            const parts = [];
            if (card.progressText.length > 0)
                parts.push(card.progressText);
            if (card.sizeText.length > 0)
                parts.push(card.sizeText);
            if (card.etaText.length > 0)
                parts.push(i18nc("@label download eta",
                    "ETA %1", card.etaText));
            return parts.length > 0
                ? parts.join(" \u00b7 ")
                : i18nc("@label download status", "Downloading\u2026");
        }
        case card.stateIdle: {
            const head = i18nc("@label download status", "Stalled");
            return card.sizeText.length > 0
                ? head + " \u00b7 " + card.sizeText
                : head;
        }
        case card.statePaused: {
            const head = i18nc("@label download status", "Paused");
            return card.sizeText.length > 0
                ? head + " \u00b7 " + card.sizeText
                : head;
        }
        case card.stateCompleted:
            return card.sizeText.length > 0
                ? i18nc("@label download status, saved + final size",
                    "Saved \u00b7 %1", card.sizeText)
                : i18nc("@label download status", "Saved");
        case card.stateFailed:
            return "\u26a0 " + (card.errorText.length > 0
                ? card.errorText
                : i18nc("@label download status", "Failed"));
        case card.stateCancelled:
            return i18nc("@label download status", "Cancelled");
        }
        return "";
    }

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

    // Map a `stateTone` string to a palette colour for the state
    // token on the meta row. Tone moves from a pill-chip border
    // (old design) to label colour so the meta row keeps a single
    // caption-sized line height for every state.
    function _stateColor(tone) {
        switch (tone) {
        case "positive": return Theme.positive;
        case "negative": return Theme.negative;
        case "warn":     return Theme.accent;
        default:         return Theme.disabled;
        }
    }

    enabled: card.imdbId.length > 0 || card.assetId.length > 0

    // Hand the progress fraction to the chassis so its built-in
    // row-level bar (between body and trailing) is the only bar on
    // the row. The chassis hides it automatically when `progress`
    // falls outside (0, 1), which covers Queued / Completed / etc.
    progress: card._progress

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

    // Body (default slot) — fixed 3-line column.

    // Line 1: title. Left-aligned bold headline; elides on narrow
    // rows and surfaces the full string via the chassis tooltip.
    QQC2.Label {
        Layout.fillWidth: true
        elide: Text.ElideRight
        text: card._headlineText
        font.weight: Font.DemiBold
        color: Theme.foreground
        onTruncatedChanged: card.titleTooltip
            = truncated ? text : ""
    }

    // Line 2: dot-separated caption tokens — State (toned),
    // Quality, Backend (icon + label), status, rate. Plain caption
    // labels and a small inline icon so the row's height stays
    // identical whether or not the optional tokens render.
    RowLayout {
        id: metaRow

        readonly property bool hasState: card.stateText.length > 0
        readonly property bool hasQuality:
            card._qualityChipText.length > 0
        readonly property bool hasBackend: card.backendLabel.length > 0
        readonly property bool hasStatus: card._statusText.length > 0
        readonly property bool hasRate: !card.complete
            && card.downloadRateText.length > 0

        Layout.fillWidth: true
        spacing: Theme.inlineSpacing

        // State — tone moves from chip border to label colour.
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: metaRow.hasState
            text: card.stateText
            font.pointSize: Theme.captionFont.pointSize
            color: card._stateColor(card.stateTone)
            verticalAlignment: Text.AlignVCenter
        }

        // (state) → (quality)
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: metaRow.hasState && metaRow.hasQuality
            text: "\u00b7"
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        // Quality (e.g. `1080p WEB-DL` / `720p`).
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: metaRow.hasQuality
            text: card._qualityChipText
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        // (state|quality) → (backend)
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: (metaRow.hasState || metaRow.hasQuality)
                && metaRow.hasBackend
            text: "\u00b7"
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        // Backend icon. Sized down from `iconSizes.small` so it
        // sits on the caption baseline next to the label rather
        // than dominating the line.
        Kirigami.Icon {
            Layout.alignment: Qt.AlignVCenter
            visible: metaRow.hasBackend
                && card.backendIcon.length > 0
            Layout.preferredWidth:
                Math.round(Kirigami.Units.iconSizes.small * 0.8)
            Layout.preferredHeight: width
            source: card.backendIcon
            color: Theme.disabled
        }

        // Backend label.
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: metaRow.hasBackend
            text: card.backendLabel
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        // (state|quality|backend) → (status)
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: (metaRow.hasState || metaRow.hasQuality
                || metaRow.hasBackend) && metaRow.hasStatus
            text: "\u00b7"
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        // Status caption — recolours `Theme.negative` on failed
        // rows (the `⚠` prefix is baked into `_statusText`).
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            Layout.fillWidth: false
            visible: metaRow.hasStatus
            text: card._statusText
            elide: Text.ElideRight
            font.pointSize: Theme.captionFont.pointSize
            color: card.state === card.stateFailed
                ? Theme.negative
                : Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        // (status) → (rate)
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: (metaRow.hasState || metaRow.hasQuality
                || metaRow.hasBackend || metaRow.hasStatus)
                && metaRow.hasRate
            text: "\u00b7"
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        // Live download rate — the only token in foreground /
        // DemiBold so it scans as the row's live metric.
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: metaRow.hasRate
            text: card.downloadRateText
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.foreground
            font.weight: Font.DemiBold
            verticalAlignment: Text.AlignVCenter
        }

        // Trailing fill keeps the row packed flush left when the
        // body stretches it to the card width.
        Item { Layout.fillWidth: true }
    }

    // Line 3: attribution — always present so the body keeps its
    // fixed line count. Falls back to a generic source string when
    // no provider / release name is known.
    QQC2.Label {
        Layout.fillWidth: true
        text: card._attributionText.length > 0
            ? card._attributionText
            : i18nc("@label download attribution unknown",
                "Unknown source")
        elide: Text.ElideRight
        font.pointSize: Theme.captionFont.pointSize
        color: Theme.disabled
    }

    // Action row: contextual primary + overflow. Both use
    // `QQC2.Button` (the filled, framed style) so they share the
    // same chrome, padding, and baseline; the overflow keeps its
    // icon-only display and so just renders as a square-ish
    // button matching the primary's height.
    trailing: RowLayout {
        spacing: Kirigami.Units.smallSpacing

        QQC2.Button {
            id: primaryButton
            visible: card._primaryKind !== card.primaryNone

            readonly property string _iconName: {
                switch (card._primaryKind) {
                case card.primaryPause:  return "pause";
                case card.primaryResume: return "play";
                case card.primaryPlay:   return "play";
                case card.primaryRetry:  return "refresh-cw";
                case card.primaryKeep:   return "download";
                }
                return "";
            }
            readonly property string _label: {
                switch (card._primaryKind) {
                case card.primaryPause:
                    return i18nc("@action:button pause this download",
                        "Pause");
                case card.primaryResume:
                    return i18nc("@action:button resume this download",
                        "Resume");
                case card.primaryPlay:
                    return i18nc("@action:button send asset to player",
                        "Play");
                case card.primaryRetry:
                    return i18nc("@action:button retry a failed download",
                        "Retry");
                case card.primaryKeep:
                    return i18nc("@action:button keep this asset "
                        + "(promote OnDemand to Full+Pinned)",
                        "Keep");
                }
                return "";
            }

            icon.source: AppIcons.url(_iconName)
            icon.color: AppIcons.controlColor(enabled, false)
            text: _label
            display: QQC2.AbstractButton.TextBesideIcon
            // Surface the "show in player" semantic for the
            // hasPlayer Active case where Play actually focuses the
            // existing playback instead of starting fresh.
            QQC2.ToolTip.text: (card._primaryKind === card.primaryPlay
                    && card.state === card.stateActive
                    && card.hasPlayerAttached)
                ? i18nc("@info:tooltip currently playing in player",
                    "Show in player")
                : _label
            QQC2.ToolTip.visible: hovered && QQC2.ToolTip.text.length > 0
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            onClicked: {
                switch (card._primaryKind) {
                case card.primaryPause:
                    downloadsVm.pauseDownload(card.assetId); break;
                case card.primaryResume:
                    downloadsVm.resumeDownload(card.assetId); break;
                case card.primaryPlay:
                    downloadsVm.playDownload(card.assetId); break;
                case card.primaryRetry:
                    downloadsVm.retry(card.assetId); break;
                case card.primaryKeep:
                    downloadsVm.upgradeToFull(card.assetId); break;
                }
            }
        }

        QQC2.Button {
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

    // Overflow menu — short, distinct verbs. Follows the
    // conventions in `docs/MenuConventions.md`: primary actions
    // at the top, copy / external next, destructive footer last.
    // Items that open a confirm dialog wire it from
    // `onTriggered` directly; the label stays short.
    KinemaMenu {
        id: rowMenu

        KinemaMenuItem {
            iconName: "play"
            label: i18nc("@action:inmenu download row", "Play")
            visible: card.complete
            onTriggered: downloadsVm.playDownload(card.assetId)
        }
        KinemaMenuItem {
            iconName: "folder-open"
            label: i18nc("@action:inmenu download row", "Open Folder")
            enabled: card.localDir.length > 0
            onTriggered: downloadsVm.openLocalDir(card.assetId)
        }
        QQC2.MenuSeparator { }
        // Pin / Unpin collapse into one stateful item per the
        // "toggle pair = one item" convention. Pin also covers the
        // OnDemand → Full + Pinned upgrade.
        KinemaMenuItem {
            iconName: card.pinned ? "circle-dashed" : "pin"
            label: card.pinned
                ? i18nc("@action:inmenu download row, allow eviction",
                    "Unpin")
                : i18nc("@action:inmenu download row, save (also "
                    + "upgrades OnDemand to Full+Pinned)",
                    "Pin")
            visible: card.pinned
                || card.canUpgrade
                || card.complete
            onTriggered: {
                if (card.pinned) {
                    downloadsVm.pin(card.assetId, false);
                } else if (card.canUpgrade && !card.complete) {
                    downloadsVm.upgradeToFull(card.assetId);
                } else {
                    downloadsVm.pin(card.assetId, true);
                }
            }
        }
        // Full+hasPlayer Pause lives only in the menu (primary slot
        // is "Play" so we don't offer Pause inline while a player
        // is attached). Confirm before pausing — the playback will
        // starve once the player catches up to cached bytes.
        KinemaMenuItem {
            iconName: "pause"
            label: i18nc("@action:inmenu download row, pause this download",
                "Pause")
            visible: card.state === card.stateActive
                && card.mode === card.modeFull
                && card.hasPlayerAttached
            onTriggered: pauseWhilePlayingConfirm.open()
        }
        QQC2.MenuSeparator { }
        KinemaMenuItem {
            iconName: "copy"
            label: i18nc("@action:inmenu download row", "Copy Title")
            enabled: card.title.length > 0
            onTriggered: shell.copyToClipboard(card.title,
                i18nc("@info:status",
                    "Title copied to clipboard"))
        }
        KinemaMenuItem {
            iconName: "copy"
            label: i18nc("@action:inmenu download row", "Copy Path")
            enabled: card.localDir.length > 0
            onTriggered: shell.copyToClipboard(card.localDir,
                i18nc("@info:status",
                    "File path copied to clipboard"))
        }
        KinemaMenuItem {
            iconName: "external-link"
            label: i18nc("@action:inmenu download row",
                "Open on IMDb")
            enabled: card.imdbId.length > 0
            onTriggered: shell.openImdbTitle(card.imdbId)
        }
        QQC2.MenuSeparator { }
        KinemaMenuItem {
            iconName: "x"
            label: i18nc("@action:inmenu download row, stop transfer",
                "Stop")
            destructive: true
            enabled: card.state !== card.stateCompleted
                && card.state !== card.stateFailed
                && card.state !== card.stateCancelled
            onTriggered: {
                if (card.hasPlayerAttached) {
                    stopWhilePlayingConfirm.open();
                } else {
                    downloadsVm.cancel(card.assetId);
                }
            }
        }
        KinemaMenuItem {
            iconName: "list-x"
            label: i18nc("@action:inmenu download row, drop the row",
                "Remove")
            destructive: true
            // Remove keeps files on disk; no confirm prompt.
            onTriggered: downloadsVm.remove(card.assetId, false)
        }
        KinemaMenuItem {
            iconName: "trash-2"
            label: i18nc("@action:inmenu download row, drop the row "
                + "and delete cached files",
                "Delete")
            destructive: true
            onTriggered: deleteConfirm.open()
        }
    }

    // Pause-while-playing confirmation (Full+hasPlayer only).
    Kirigami.PromptDialog {
        id: pauseWhilePlayingConfirm
        title: i18nc("@title:dialog", "Pause download?")
        subtitle: i18nc("@info confirm pause while player attached",
            "The player is currently reading from this download. "
            + "Pausing will let it run until it catches up to the "
            + "cached bytes, then playback will buffer-starve.")
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: i18nc("@action:button", "Cancel")
                onTriggered: pauseWhilePlayingConfirm.close()
            },
            Kirigami.Action {
                text: i18nc("@action:button pause anyway", "Pause")
                icon.source: AppIcons.url("pause")
                onTriggered: {
                    downloadsVm.pauseDownload(card.assetId);
                    pauseWhilePlayingConfirm.close();
                }
            }
        ]
    }

    // Stop-while-playing confirmation. For OnDemand+hasPlayer this
    // also ends playback; for Full+hasPlayer the cached bytes
    // remain and playback continues until they run out.
    Kirigami.PromptDialog {
        id: stopWhilePlayingConfirm
        title: i18nc("@title:dialog", "Stop transfer?")
        subtitle: i18nc("@info confirm stop while player attached",
            "The player is currently reading from this download. "
            + "Stopping the transfer will end playback once the "
            + "player runs out of cached bytes.")
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: i18nc("@action:button", "Cancel")
                onTriggered: stopWhilePlayingConfirm.close()
            },
            Kirigami.Action {
                text: i18nc("@action:button stop anyway", "Stop")
                icon.source: AppIcons.url("x")
                onTriggered: {
                    downloadsVm.cancel(card.assetId);
                    stopWhilePlayingConfirm.close();
                }
            }
        ]
    }

    Kirigami.PromptDialog {
        id: deleteConfirm
        title: i18nc("@title:dialog", "Delete cached files?")
        subtitle: card.hasPlayerAttached
            ? i18nc("@info confirm delete while player attached",
                "The player is currently reading from this download. "
                + "Removing and deleting the cached files will end "
                + "playback. This cannot be undone.")
            : i18nc(
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
