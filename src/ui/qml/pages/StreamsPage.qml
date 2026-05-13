// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

// Pushed-on-top streams surface. `MainController::showStreamsRequested(detailVm)`
// can reach it either from an active detail page or directly from
// Continue Watching. The page is VM-agnostic — both
// `MovieDetailViewModel` and `SeriesDetailViewModel` expose the same
// contract used by `StreamsList`.
//
// Chrome:
//
//   * `header:` is a single merged `PageHeaderBar` inlining the page
//     title with a single `Sort` trigger button (opens
//     `StreamSortDialog`) and a single `Filters` button.
//   * The filters dialog owns all stream filters — Resolution,
//     Cached only, HDR, Dolby Vision, and Dual / Multi audio.
//   * The sort dialog owns the sort mode and ascending / descending
//     direction in one place, so the header trigger can stay short
//     even for `Smart` (which would otherwise spell out its three-key
//     shape inline).
//
// Horizontal padding follows the app-wide rule: the body
// `StreamsList` (a `ListSurface`) spans the page edge-to-edge,
// and `StreamListCard` rows inherit the canonical
// `Theme.pageMargin` content inset via `BaseListCard`'s own
// internal padding. The section header ("1080P" / "Other")
// likewise spans full width with its text inset by
// `Theme.pageMargin`, so card content, section labels, and the
// `PageHeaderBar` title all line up on the same vertical axis.
//
// Esc pops back to the previous page (handled by the shell-level
// shortcut in `ApplicationShell.qml`).
Kirigami.Page {
    id: page

    objectName: "streams"
    leftPadding: 0
    rightPadding: 0
    topPadding: 0
    bottomPadding: 0

    // Set by the shell when the page is created. Kept as `var` rather
    // than a typed property so QML accepts both detail VM types
    // interchangeably.
    property var detailVm

    // Right-most header action. Re-runs only the streams fetch (cheaper
    // than `retry()`, which also re-fetches the meta). Rendered via
    // `PageHeaderBar.pageActions` so it lands after the "Filters"
    // button, matching Browse / Library / Discover. Disabled while a
    // fetch is in flight so users can't double-tap during a refresh.
    actions: [
        Kirigami.Action {
            icon.source: AppIcons.url("refresh-cw")
            icon.color: AppIcons.foreground
            text: i18nc("@action:button refresh streams", "Refresh")
            displayHint: Kirigami.DisplayHint.IconOnly
            shortcut: StandardKey.Refresh
            enabled: page.detailVm
                && page.detailVm.streams
                && page.detailVm.streams.state !== StreamsListModel.Loading
            onTriggered: if (page.detailVm) page.detailVm.refreshStreams()
        }
    ]

    // Title carries the title / episode label, with the visible-row
    // count appended after a separator when there are streams.
    title: {
        if (!page.detailVm) {
            return i18nc("@title:window streams page fallback", "Streams");
        }
        const label = (page.detailVm.selectedEpisodeLabel !== undefined
            && page.detailVm.selectedEpisodeLabel.length > 0)
            // Series — `selectedEpisodeLabel` already includes show title.
            ? page.detailVm.selectedEpisodeLabel
            : page.detailVm.title;
        if (page.detailVm.streams
            && page.detailVm.streams.count > 0) {
            return i18nc(
                "@title:window streams page, %1 is title, %2 is count",
                "%1 \u00b7 %2 streams", label,
                page.detailVm.streams.count);
        }
        return label;
    }

    // ---- filters dialog -----------------------------------------
    Kirigami.Dialog {
        id: streamsAdvancedDialog
        title: i18nc("@title:dialog stream filters", "Filters")
        preferredWidth: Kirigami.Units.gridUnit * 26

        readonly property var resolutionValues: ["", "2160p", "1080p", "720p", "sd"]
        readonly property var resolutionLabels: [
            i18nc("@item resolution", "Any"),
            i18nc("@item resolution", "4K"),
            i18nc("@item resolution", "1080p"),
            i18nc("@item resolution", "720p"),
            i18nc("@item resolution", "SD")
        ]

        footer: QQC2.DialogButtonBox {
            QQC2.Button {
                text: i18nc("@action:button reset all stream filters",
                    "Reset all filters")
                icon.source: AppIcons.url("eraser")
                icon.color: AppIcons.foreground
                visible: page.detailVm
                    && (page.detailVm.uiResolutionFilter.length > 0
                        || page.detailVm.uiHdrOnly
                        || page.detailVm.uiDolbyVisionOnly
                        || page.detailVm.uiMultiAudioOnly)
                QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.ActionRole
                onClicked: {
                    if (page.detailVm) {
                        page.detailVm.clearUiFilters();
                    }
                    streamsAdvancedDialog.close();
                }
            }
            QQC2.Button {
                text: i18nc("@action:button close dialog", "Close")
                QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.AcceptRole
                onClicked: streamsAdvancedDialog.close()
            }
        }

        FormCard.FormCard {
            FormCard.FormComboBoxDelegate {
                text: i18nc("@label stream filter", "Resolution")
                model: streamsAdvancedDialog.resolutionLabels
                currentIndex: Math.max(0,
                    streamsAdvancedDialog.resolutionValues.indexOf(
                        page.detailVm ? page.detailVm.uiResolutionFilter : ""))
                onActivated: idx => {
                    if (page.detailVm) {
                        page.detailVm.uiResolutionFilter =
                            streamsAdvancedDialog.resolutionValues[idx];
                    }
                }
            }
            FormCard.FormSwitchDelegate {
                text: i18nc("@option:check stream filter", "HDR")
                description: i18nc("@info:tooltip stream filter",
                    "Streams with any HDR profile (HDR10, HDR10+, Dolby Vision).")
                checked: page.detailVm && page.detailVm.uiHdrOnly
                onToggled: if (page.detailVm) {
                    page.detailVm.uiHdrOnly = checked;
                }
            }
            FormCard.FormSwitchDelegate {
                text: i18nc("@option:check stream filter",
                    "Dolby Vision")
                checked: page.detailVm
                    && page.detailVm.uiDolbyVisionOnly
                onToggled: if (page.detailVm) {
                    page.detailVm.uiDolbyVisionOnly = checked;
                }
            }
            FormCard.FormSwitchDelegate {
                text: i18nc("@option:check stream filter", "Dual / Multi audio")
                checked: page.detailVm
                    && page.detailVm.uiMultiAudioOnly
                onToggled: if (page.detailVm) {
                    page.detailVm.uiMultiAudioOnly = checked;
                }
            }
        }
    }

    // ---- sort dialog -------------------------------------------
    StreamSortDialog {
        id: streamSortDialog
        vm: page.detailVm
    }

    // ---- header: merged title + sorting + filters + refresh -----
    header: PageHeaderBar {
        id: filterBar
        title: page.title
        pageActions: page.actions
        advancedFiltersDialog: streamsAdvancedDialog
        advancedFiltersButtonText: i18nc(
            "@action:button open stream filters dialog", "Filters")
        advancedFilterCount: page.detailVm
            ? ((page.detailVm.uiResolutionFilter.length > 0 ? 1 : 0)
                + (page.detailVm.uiHdrOnly ? 1 : 0)
                + (page.detailVm.uiDolbyVisionOnly ? 1 : 0)
                + (page.detailVm.uiMultiAudioOnly ? 1 : 0))
            : 0

        // Right-align controls after the title.
        Item { Layout.fillWidth: true }

        // ---- Sort trigger ---------------------------------------
        // Single button — opens StreamSortDialog. Label intentionally
        // omits Smart's parenthetical shape ("cached, quality,
        // seeders") because the dialog body explains it; the bar has
        // to fit. For non-Smart modes the trailing arrow signals the
        // active direction so the user does not have to open the
        // dialog to read it.
        QQC2.ToolButton {
            Layout.alignment: Qt.AlignVCenter
            flat: true
            display: QQC2.AbstractButton.TextBesideIcon
            icon.source: AppIcons.url("arrow-up-down")
            readonly property int mode: page.detailVm
                ? page.detailVm.sortMode : StreamsListModel.Smart
            readonly property bool isSmart:
                mode === StreamsListModel.Smart
            readonly property bool active: !isSmart
            icon.color: active
                ? AppIcons.accent
                : AppIcons.controlColor(enabled, false)
            font.bold: active
            enabled: page.detailVm
                && page.detailVm.streams
                && page.detailVm.streams.count > 0
            text: {
                const m = mode;
                const arrow = isSmart
                    ? ""
                    : (page.detailVm && page.detailVm.sortDescending
                        ? " \u2193" : " \u2191");
                switch (m) {
                case StreamsListModel.Smart:
                    return i18nc("@action:button sort trigger label",
                        "Sort: Smart");
                case StreamsListModel.Seeders:
                    return i18nc(
                        "@action:button sort trigger, %1 direction arrow",
                        "Sort: Seeders%1", arrow);
                case StreamsListModel.Size:
                    return i18nc(
                        "@action:button sort trigger, %1 direction arrow",
                        "Sort: Size%1", arrow);
                case StreamsListModel.Quality:
                    return i18nc(
                        "@action:button sort trigger, %1 direction arrow",
                        "Sort: Quality%1", arrow);
                case StreamsListModel.Provider:
                    return i18nc(
                        "@action:button sort trigger, %1 direction arrow",
                        "Sort: Provider%1", arrow);
                case StreamsListModel.ReleaseName:
                    return i18nc(
                        "@action:button sort trigger, %1 direction arrow",
                        "Sort: Release name%1", arrow);
                }
                return i18nc("@action:button sort trigger fallback",
                    "Sort");
            }
            QQC2.ToolTip.text: i18nc("@info:tooltip sort trigger",
                "Choose how streams are ordered")
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            onClicked: streamSortDialog.open()
        }
    }

    // ---- body: streams list ------------------------------------
    StreamsList {
        anchors.fill: parent
        visible: true
        vm: page.detailVm
    }
}
