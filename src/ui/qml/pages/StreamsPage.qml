// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

// Pushed-on-top streams surface. Each detail page (movie / series)
// reaches this page through `MainController::showStreamsRequested(detailVm)`.
// The page is VM-agnostic — both `MovieDetailViewModel` and
// `SeriesDetailViewModel` expose the same contract used by `StreamsList`.
//
// Chrome:
//
//   * `header:` is a single merged `PageHeaderBar` inlining the page
//     title with sorting affordances (`Sort ▾` + `Descending`) and a
//     single `Filters` button.
//   * The filters dialog owns all stream filters — Resolution,
//     Cached only, HDR, Dolby Vision, and Dual / Multi audio — so the
//     header stays compact.
//
// Esc pops back to the detail page (handled by the shell-level
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
                icon.name: "edit-clear-history"
                visible: page.detailVm
                    && (page.detailVm.cachedOnly
                        || page.detailVm.uiResolutionFilter.length > 0
                        || page.detailVm.uiHdrOnly
                        || page.detailVm.uiDolbyVisionOnly
                        || page.detailVm.uiMultiAudioOnly)
                QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.ActionRole
                onClicked: {
                    if (page.detailVm) {
                        page.detailVm.cachedOnly = false;
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
                text: i18nc("@option:check stream filter", "Cached only")
                description: i18nc("@info:tooltip stream filter",
                    "Show only streams already cached on Real-Debrid — they play instantly.")
                visible: page.detailVm
                    && page.detailVm.realDebridConfigured
                    && page.detailVm.rawStreamsCount > 0
                checked: page.detailVm && page.detailVm.cachedOnly
                onToggled: if (page.detailVm) {
                    page.detailVm.cachedOnly = checked;
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

    // ---- header: merged title + sorting + filters ---------------
    header: PageHeaderBar {
        id: filterBar
        title: page.title
        advancedFiltersDialog: streamsAdvancedDialog
        advancedFiltersButtonText: i18nc(
            "@action:button open stream filters dialog", "Filters")
        advancedFilterCount: page.detailVm
            ? ((page.detailVm.cachedOnly ? 1 : 0)
                + (page.detailVm.uiResolutionFilter.length > 0 ? 1 : 0)
                + (page.detailVm.uiHdrOnly ? 1 : 0)
                + (page.detailVm.uiDolbyVisionOnly ? 1 : 0)
                + (page.detailVm.uiMultiAudioOnly ? 1 : 0))
            : 0

        // Right-align controls after the title.
        Item { Layout.fillWidth: true }

        // ---- Sort (pick-one) ------------------------------------
        FilterMenuButton {
            Layout.alignment: Qt.AlignVCenter
            axisLabel: i18nc("@action:button browse sort", "Sort")
            icon.name: "view-sort"
            active: page.detailVm
                && page.detailVm.sortMode !== StreamsListModel.Smart
            enabled: page.detailVm
                && page.detailVm.streams
                && page.detailVm.streams.count > 0
            options: [
                { value: StreamsListModel.Smart, label:
                    i18nc("@action:inmenu sort streams",
                        "Smart (cached, then quality)") },
                { value: StreamsListModel.Seeders, label:
                    i18nc("@action:inmenu sort streams", "Seeders") },
                { value: StreamsListModel.Size, label:
                    i18nc("@action:inmenu sort streams", "Size") },
                { value: StreamsListModel.Quality, label:
                    i18nc("@action:inmenu sort streams", "Quality") },
                { value: StreamsListModel.Provider, label:
                    i18nc("@action:inmenu sort streams", "Provider") },
                { value: StreamsListModel.ReleaseName, label:
                    i18nc("@action:inmenu sort streams",
                        "Release name") }
            ]
            currentValue: page.detailVm
                ? page.detailVm.sortMode : StreamsListModel.Smart
            onActivated: v => {
                if (page.detailVm) {
                    page.detailVm.sortMode = v;
                }
            }
        }

        // ---- Descending toggle ----------------------------------
        // Pulled out of the Sort menu so the affordance is visible
        // at a glance. Disabled when sort mode is Smart, which has
        // a fixed shape.
        QQC2.ToolButton {
            Layout.alignment: Qt.AlignVCenter
            display: QQC2.AbstractButton.IconOnly
            icon.name: "view-sort-descending"
            flat: true
            checkable: true
            text: i18nc("@action:button toggle descending sort",
                "Descending")
            QQC2.ToolTip.text: text
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            enabled: page.detailVm
                && page.detailVm.sortMode !== StreamsListModel.Smart
            checked: page.detailVm && page.detailVm.sortDescending
            onClicked: if (page.detailVm) {
                page.detailVm.sortDescending =
                    !page.detailVm.sortDescending;
            }
        }
    }

    // ---- body: streams list ------------------------------------
    StreamsList {
        anchors.fill: parent
        visible: true
        vm: page.detailVm
    }
}
