// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.components as Components

import dev.tlmtech.kinema.app

// Pushed-on-top streams surface. Each detail page (movie / series)
// reaches this page through `MainController::showStreamsRequested(detailVm)`.
// The page is VM-agnostic — both `MovieDetailViewModel` and
// `SeriesDetailViewModel` expose the same contract used by `StreamsList`.
//
// Chrome:
//
//   * `header:` is a `QQC2.ToolBar` (Header colorSet) pairing a
//     `Components.SegmentedButton` (mutually exclusive resolution choice
//     — 4K / 1080p / 720p / SD) with a `Kirigami.ActionToolBar` of
//     toggle filters (Cached only, HDR, Dolby Vision, Multi audio) and
//     a Sort ▾ parent action whose children carry the 6 sort modes
//     plus the Descending toggle.
//   * Below the header, an inline chip strip lists active filters as
//     removable `Kirigami.Chip`s with a trailing "Clear all".
//   * The Subtitles… affordance stays as a top-level page action.
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

    // ---- active-filter chips (derived from VM ui* properties) ----
    // Synthesised in QML rather than as a Q_PROPERTY on the detail
    // VMs; the chip set is short and the labels are inline-i18n'd.
    readonly property var streamsChips: {
        if (!page.detailVm) return [];
        const out = [];
        if (page.detailVm.cachedOnly) {
            out.push({
                kind: "cached",
                label: i18nc("@label streams chip", "Cached only")
            });
        }
        if (page.detailVm.uiResolutionFilter
            && page.detailVm.uiResolutionFilter.length > 0) {
            const r = page.detailVm.uiResolutionFilter;
            const upper = (r === "sd")
                ? i18nc("@label streams chip resolution", "SD")
                : (r === "2160p")
                    ? i18nc("@label streams chip resolution", "4K")
                    : r.toUpperCase();
            out.push({ kind: "res", label: upper });
        }
        if (page.detailVm.uiHdrOnly) {
            out.push({
                kind: "hdr",
                label: i18nc("@label streams chip", "HDR")
            });
        }
        if (page.detailVm.uiDolbyVisionOnly) {
            out.push({
                kind: "dv",
                label: i18nc("@label streams chip", "Dolby Vision")
            });
        }
        if (page.detailVm.uiMultiAudioOnly) {
            out.push({
                kind: "ma",
                label: i18nc("@label streams chip", "Multi audio")
            });
        }
        return out;
    }

    function removeStreamsChip(idx) {
        if (!page.detailVm || idx < 0 || idx >= page.streamsChips.length) {
            return;
        }
        const chip = page.streamsChips[idx];
        switch (chip.kind) {
        case "cached":  page.detailVm.cachedOnly = false; break;
        case "res":     page.detailVm.uiResolutionFilter = ""; break;
        case "hdr":     page.detailVm.uiHdrOnly = false; break;
        case "dv":      page.detailVm.uiDolbyVisionOnly = false; break;
        case "ma":      page.detailVm.uiMultiAudioOnly = false; break;
        }
    }

    function clearAllStreamsFilters() {
        if (!page.detailVm) return;
        page.detailVm.cachedOnly = false;
        page.detailVm.clearUiFilters();
    }

    actions: [
        Kirigami.Action {
            icon.name: "subtitles"
            text: i18nc("@action:button", "Subtitles\u2026")
            enabled: page.detailVm !== undefined
                && page.detailVm !== null
            onTriggered: page.detailVm.requestSubtitles()
        }
    ]

    // ---- header: filter toolbar ---------------------------------
    header: QQC2.ToolBar {
        id: filterBar

        Kirigami.Theme.colorSet: Kirigami.Theme.Header
        Kirigami.Theme.inherit: false

        leftPadding: Theme.pageMargin
        rightPadding: Theme.pageMargin
        topPadding: Theme.inlineSpacing
        bottomPadding: Theme.inlineSpacing

        contentItem: RowLayout {
            spacing: Theme.groupSpacing

            // ---- Resolution (mutually exclusive) ----------------
            // SegmentedButton communicates "pick one" visually. The
            // VM exposes a single `uiResolutionFilter` string; clicking
            // the active chip again clears the axis (sets it to "").
            Components.SegmentedButton {
                Layout.alignment: Qt.AlignVCenter
                actions: [
                    Kirigami.Action {
                        text: i18nc("@option:radio stream filter resolution",
                            "4K")
                        checkable: true
                        checked: page.detailVm
                            && page.detailVm.uiResolutionFilter === "2160p"
                        onTriggered: if (page.detailVm) {
                            page.detailVm.uiResolutionFilter =
                                page.detailVm.uiResolutionFilter === "2160p"
                                    ? "" : "2160p";
                        }
                    },
                    Kirigami.Action {
                        text: i18nc("@option:radio stream filter resolution",
                            "1080p")
                        checkable: true
                        checked: page.detailVm
                            && page.detailVm.uiResolutionFilter === "1080p"
                        onTriggered: if (page.detailVm) {
                            page.detailVm.uiResolutionFilter =
                                page.detailVm.uiResolutionFilter === "1080p"
                                    ? "" : "1080p";
                        }
                    },
                    Kirigami.Action {
                        text: i18nc("@option:radio stream filter resolution",
                            "720p")
                        checkable: true
                        checked: page.detailVm
                            && page.detailVm.uiResolutionFilter === "720p"
                        onTriggered: if (page.detailVm) {
                            page.detailVm.uiResolutionFilter =
                                page.detailVm.uiResolutionFilter === "720p"
                                    ? "" : "720p";
                        }
                    },
                    Kirigami.Action {
                        text: i18nc("@option:radio stream filter resolution",
                            "SD")
                        checkable: true
                        checked: page.detailVm
                            && page.detailVm.uiResolutionFilter === "sd"
                        onTriggered: if (page.detailVm) {
                            page.detailVm.uiResolutionFilter =
                                page.detailVm.uiResolutionFilter === "sd"
                                    ? "" : "sd";
                        }
                    }
                ]
            }

            // ---- toggles + Sort -------------------------------------
            Kirigami.ActionToolBar {
                Layout.fillWidth: true
                alignment: Qt.AlignLeft
                flat: true

                actions: [
                    Kirigami.Action {
                        text: i18nc("@option:check stream filter",
                            "Cached only")
                        icon.name: "package-installed-outdated"
                        checkable: true
                        visible: page.detailVm
                            && page.detailVm.realDebridConfigured
                            && page.detailVm.rawStreamsCount > 0
                        checked: page.detailVm
                            && page.detailVm.cachedOnly
                        tooltip: i18nc("@info:tooltip stream filter",
                            "Show only streams already cached on Real-Debrid \u2014 they play instantly.")
                        onTriggered: if (page.detailVm) {
                            page.detailVm.cachedOnly = !page.detailVm.cachedOnly;
                        }
                    },
                    Kirigami.Action {
                        text: i18nc("@option:check stream filter", "HDR")
                        checkable: true
                        checked: page.detailVm && page.detailVm.uiHdrOnly
                        tooltip: i18nc("@info:tooltip stream filter",
                            "Streams with any HDR profile (HDR10, HDR10+, Dolby Vision).")
                        onTriggered: if (page.detailVm) {
                            page.detailVm.uiHdrOnly = !page.detailVm.uiHdrOnly;
                        }
                    },
                    Kirigami.Action {
                        text: i18nc("@option:check stream filter", "Dolby Vision")
                        checkable: true
                        checked: page.detailVm
                            && page.detailVm.uiDolbyVisionOnly
                        onTriggered: if (page.detailVm) {
                            page.detailVm.uiDolbyVisionOnly =
                                !page.detailVm.uiDolbyVisionOnly;
                        }
                    },
                    Kirigami.Action {
                        text: i18nc("@option:check stream filter",
                            "Dual / Multi audio")
                        checkable: true
                        checked: page.detailVm
                            && page.detailVm.uiMultiAudioOnly
                        onTriggered: if (page.detailVm) {
                            page.detailVm.uiMultiAudioOnly =
                                !page.detailVm.uiMultiAudioOnly;
                        }
                    },

                    // ---- Sort ▾ (pick-one + Descending toggle) -------
                    Kirigami.Action {
                        text: i18nc("@action:button browse sort", "Sort")
                        icon.name: "view-sort"
                        enabled: page.detailVm
                            && page.detailVm.streams
                            && page.detailVm.streams.count > 0
                        children: [
                            Kirigami.Action {
                                text: i18nc("@action:inmenu sort streams",
                                    "Smart (cached, then quality)")
                                checkable: true
                                checked: page.detailVm
                                    && page.detailVm.sortMode === StreamsListModel.Smart
                                onTriggered: if (page.detailVm) {
                                    page.detailVm.sortMode = StreamsListModel.Smart;
                                }
                            },
                            Kirigami.Action {
                                separator: true
                            },
                            Kirigami.Action {
                                text: i18nc("@action:inmenu sort streams", "Seeders")
                                checkable: true
                                checked: page.detailVm
                                    && page.detailVm.sortMode === StreamsListModel.Seeders
                                onTriggered: if (page.detailVm) {
                                    page.detailVm.sortMode = StreamsListModel.Seeders;
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@action:inmenu sort streams", "Size")
                                checkable: true
                                checked: page.detailVm
                                    && page.detailVm.sortMode === StreamsListModel.Size
                                onTriggered: if (page.detailVm) {
                                    page.detailVm.sortMode = StreamsListModel.Size;
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@action:inmenu sort streams", "Quality")
                                checkable: true
                                checked: page.detailVm
                                    && page.detailVm.sortMode === StreamsListModel.Quality
                                onTriggered: if (page.detailVm) {
                                    page.detailVm.sortMode = StreamsListModel.Quality;
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@action:inmenu sort streams", "Provider")
                                checkable: true
                                checked: page.detailVm
                                    && page.detailVm.sortMode === StreamsListModel.Provider
                                onTriggered: if (page.detailVm) {
                                    page.detailVm.sortMode = StreamsListModel.Provider;
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@action:inmenu sort streams",
                                    "Release name")
                                checkable: true
                                checked: page.detailVm
                                    && page.detailVm.sortMode === StreamsListModel.ReleaseName
                                onTriggered: if (page.detailVm) {
                                    page.detailVm.sortMode = StreamsListModel.ReleaseName;
                                }
                            },
                            Kirigami.Action {
                                separator: true
                            },
                            Kirigami.Action {
                                text: i18nc("@action:inmenu sort streams",
                                    "Descending")
                                checkable: true
                                checked: page.detailVm
                                    && page.detailVm.sortDescending
                                // Smart has a fixed shape — disable the
                                // toggle so the user doesn't think they're
                                // affecting anything when they aren't.
                                enabled: page.detailVm
                                    && page.detailVm.sortMode !== StreamsListModel.Smart
                                onTriggered: if (page.detailVm) {
                                    page.detailVm.sortDescending =
                                        !page.detailVm.sortDescending;
                                }
                            }
                        ]
                    }
                ]
            }
        }
    }

    // ---- body: chip strip + state-switched streams list ---------
    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // ---- active-filter chip strip ---------------------------
        Item {
            id: chipStrip

            Layout.fillWidth: true
            visible: page.streamsChips.length > 0
            implicitHeight: visible
                ? chipRow.implicitHeight + Theme.inlineSpacing
                : 0

            Behavior on implicitHeight {
                NumberAnimation { duration: Kirigami.Units.shortDuration }
            }

            RowLayout {
                id: chipRow
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                anchors.leftMargin: Theme.pageMargin
                anchors.rightMargin: Theme.pageMargin
                spacing: Theme.inlineSpacing

                QQC2.Label {
                    text: i18nc("@label prefix for active filter chip list",
                        "Filters:")
                    color: Kirigami.Theme.disabledTextColor
                }

                QQC2.ScrollView {
                    Layout.fillWidth: true
                    Layout.preferredHeight: chipFlow.implicitHeight
                    contentWidth: chipFlow.implicitWidth
                    contentHeight: chipFlow.implicitHeight
                    QQC2.ScrollBar.horizontal.policy: QQC2.ScrollBar.AsNeeded
                    QQC2.ScrollBar.vertical.policy: QQC2.ScrollBar.AlwaysOff
                    clip: true

                    Row {
                        id: chipFlow
                        spacing: Theme.inlineSpacing

                        Repeater {
                            model: page.streamsChips

                            delegate: Kirigami.Chip {
                                required property int index
                                required property var modelData
                                text: modelData.label !== undefined
                                    ? modelData.label : ""
                                closable: true
                                checkable: false
                                Accessible.name: i18nc("@info:whatsthis",
                                    "Remove filter %1", text)
                                onRemoved: page.removeStreamsChip(index)
                            }
                        }
                    }
                }

                QQC2.ToolButton {
                    text: i18nc("@action:button clear every active filter",
                        "Clear all")
                    icon.name: "edit-clear-history"
                    display: QQC2.AbstractButton.TextBesideIcon
                    onClicked: page.clearAllStreamsFilters()
                }
            }
        }

        StreamsList {
            Layout.fillWidth: true
            Layout.fillHeight: true
            vm: page.detailVm
        }
    }
}
