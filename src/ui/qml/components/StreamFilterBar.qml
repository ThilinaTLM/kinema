// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Quick-filter chip strip shown above the streams list. Mirrors the
// idiom from `BrowseFilterBar` \u2014 native Kirigami/Plasma components,
// no custom palette, no hard-coded sizes.
//
// Bound to a detail view-model (movie / series) that exposes the
// transient `uiResolutionFilter` / `uiHdrOnly` / `uiDolbyVisionOnly`
// / `uiMultiAudioOnly` properties + a `cachedOnly` toggle.
//
// The chips are toggle buttons, not popups: tapping a resolution
// chip activates that filter (and clears any other resolution chip
// since they are mutually exclusive); tapping HDR / DV / multi-audio
// independently toggles those axes.
//
// Resolution chips are always visible. The "Cached only" chip
// hides itself when Real-Debrid isn't configured or there are no
// raw streams to operate on yet \u2014 same gating as the legacy
// header checkbox.
Item {
    id: bar

    /// View-model with the streams + UI filter Q_PROPERTYs. Defaults
    /// to `movieDetailVm`; the series page rebinds to its own VM.
    property var vm: movieDetailVm

    implicitHeight: flow.implicitHeight
        + Kirigami.Units.smallSpacing * 2

    // Wrap to a second line on narrow widths instead of truncating.
    Flow {
        id: flow
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: Kirigami.Units.largeSpacing
        anchors.rightMargin: Kirigami.Units.largeSpacing
        anchors.verticalCenter: parent.verticalCenter
        spacing: Kirigami.Units.smallSpacing

        // ---- Cached on RD --------------------------------------
        QQC2.Button {
            text: i18nc("@option:check stream filter", "Cached only")
            icon.name: "package-installed-outdated"
            checkable: true
            flat: true
            visible: bar.vm.realDebridConfigured
                && bar.vm.rawStreamsCount > 0
            checked: bar.vm.cachedOnly
            onToggled: bar.vm.cachedOnly = checked
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.text: i18nc(
                "@info:tooltip stream filter",
                "Show only streams already cached on Real-Debrid \u2014 they play instantly.")
        }

        // ---- Resolution (mutually exclusive) -------------------
        // Implemented as four flat checkable buttons mapped to the
        // VM's single-value `uiResolutionFilter` property.
        Repeater {
            model: [
                { id: "2160p", label: i18nc("@option:check stream filter", "4K") },
                { id: "1080p", label: i18nc("@option:check stream filter", "1080p") },
                { id: "720p",  label: i18nc("@option:check stream filter", "720p") },
                { id: "sd",    label: i18nc("@option:check stream filter", "SD") }
            ]
            delegate: QQC2.Button {
                required property var modelData
                text: modelData.label
                checkable: true
                flat: true
                checked: bar.vm.uiResolutionFilter === modelData.id
                onToggled: {
                    if (checked) {
                        bar.vm.uiResolutionFilter = modelData.id;
                    } else if (bar.vm.uiResolutionFilter === modelData.id) {
                        // Untoggling the active chip clears the axis.
                        bar.vm.uiResolutionFilter = "";
                    }
                }
            }
        }

        // ---- HDR / Dolby Vision / Multi audio ------------------
        QQC2.Button {
            text: i18nc("@option:check stream filter", "HDR")
            checkable: true
            flat: true
            checked: bar.vm.uiHdrOnly
            onToggled: bar.vm.uiHdrOnly = checked
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.text: i18nc("@info:tooltip stream filter",
                "Streams with any HDR profile (HDR10, HDR10+, Dolby Vision).")
        }
        QQC2.Button {
            text: i18nc("@option:check stream filter", "Dolby Vision")
            checkable: true
            flat: true
            checked: bar.vm.uiDolbyVisionOnly
            onToggled: bar.vm.uiDolbyVisionOnly = checked
        }
        QQC2.Button {
            text: i18nc("@option:check stream filter",
                "Dual / Multi audio")
            checkable: true
            flat: true
            checked: bar.vm.uiMultiAudioOnly
            onToggled: bar.vm.uiMultiAudioOnly = checked
        }
    }

    // ---- Reset link --------------------------------------------
    // Anchored separately so it stays right-aligned even when the
    // chips wrap onto a second line.
    QQC2.ToolButton {
        anchors.right: parent.right
        anchors.rightMargin: Kirigami.Units.largeSpacing
        anchors.top: parent.top
        anchors.topMargin: Kirigami.Units.smallSpacing
        text: i18nc("@action:button reset stream filters", "Reset")
        icon.name: "edit-clear-all"
        display: QQC2.AbstractButton.TextBesideIcon
        visible: bar.vm.uiAnyFilterActive
        onClicked: bar.vm.clearUiFilters()
    }
}
