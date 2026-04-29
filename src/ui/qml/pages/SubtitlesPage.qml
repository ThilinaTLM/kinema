// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

// Subtitles search/download page.
//
// Chrome:
//
//   * `header:` is a single merged `PageHeaderBar` inlining the page
//     title with the basic filters — `Kirigami.SearchField` bound to
//     `subtitlesVm.release` and a `Languages ▾` multi-select — plus
//     a `More filters… (N)` button that opens the
//     `subtitlesAdvancedDialog` carrying HI / Foreign-parts-only.
//   * Below the header, an inline chip strip lists active filters as
//     removable `Kirigami.Chip`s with a trailing "Clear all".
//   * Body is `SubtitlesPanel` (status line + results list + footer
//     toolbar with Open file… / Cancel / primary action).
Kirigami.Page {
    id: page

    objectName: "subtitles"
    leftPadding: 0
    rightPadding: 0
    topPadding: 0
    bottomPadding: 0
    title: i18nc("@title:window subtitles search page",
        "Subtitles for %1", subtitlesVm.contextTitle)

    actions: [
        Kirigami.Action {
            icon.name: "edit-find"
            text: i18nc("@action:button subtitles run search", "Search")
            displayHint: Kirigami.DisplayHint.IconOnly
            enabled: subtitlesVm.state !== "notconfigured"
            onTriggered: subtitlesVm.runSearch()
        },
        Kirigami.Action {
            icon.name: "configure"
            text: i18nc("@action subtitles page header, open settings",
                "Open settings…")
            onTriggered: subtitlesVm.openSettings()
        }
    ]

    // ---- active-filter chips (derived from VM filter properties) -
    readonly property var subtitlesChips: {
        const out = [];
        const langs = subtitlesVm.languages || [];
        for (let i = 0; i < langs.length; ++i) {
            const c = langs[i];
            out.push({
                kind: "lang",
                label: c.toUpperCase(),
                payload: c
            });
        }
        if (subtitlesVm.hi !== "off") {
            out.push({
                kind: "hi",
                label: subtitlesVm.hi === "only"
                    ? i18nc("@label subtitles chip", "HI only")
                    : i18nc("@label subtitles chip", "HI included")
            });
        }
        if (subtitlesVm.fpo !== "off") {
            out.push({
                kind: "fpo",
                label: subtitlesVm.fpo === "only"
                    ? i18nc("@label subtitles chip", "Foreign-only only")
                    : i18nc("@label subtitles chip", "Foreign-only included")
            });
        }
        if (subtitlesVm.release && subtitlesVm.release.length > 0) {
            out.push({
                kind: "release",
                label: i18nc("@label subtitles chip, %1 is the user-supplied release-name fragment",
                    "Release: %1", subtitlesVm.release)
            });
        }
        return out;
    }

    function removeSubtitlesChip(idx) {
        if (idx < 0 || idx >= page.subtitlesChips.length) {
            return;
        }
        const chip = page.subtitlesChips[idx];
        switch (chip.kind) {
        case "lang": {
            const next = (subtitlesVm.languages || []).slice();
            const at = next.indexOf(chip.payload);
            if (at >= 0) {
                next.splice(at, 1);
                subtitlesVm.languages = next;
            }
            break;
        }
        case "hi":      subtitlesVm.hi = "off"; break;
        case "fpo":     subtitlesVm.fpo = "off"; break;
        case "release": subtitlesVm.release = ""; break;
        }
    }

    // ---- advanced filters dialog --------------------------------
    Kirigami.Dialog {
        id: subtitlesAdvancedDialog
        title: i18nc("@title:dialog subtitles advanced filters",
            "More filters")
        standardButtons: Kirigami.Dialog.Close
        preferredWidth: Kirigami.Units.gridUnit * 26

        readonly property var modes: ["off", "include", "only"]
        readonly property var modeLabels: [
            i18nc("@option:radio subtitle filter mode", "Off"),
            i18nc("@option:radio subtitle filter mode", "Include"),
            i18nc("@option:radio subtitle filter mode", "Only")
        ]

        FormCard.FormCard {
            FormCard.FormComboBoxDelegate {
                text: i18nc("@label subtitles filter",
                    "Hearing impaired")
                model: subtitlesAdvancedDialog.modeLabels
                currentIndex: Math.max(0,
                    subtitlesAdvancedDialog.modes.indexOf(subtitlesVm.hi))
                onActivated: idx => {
                    const v = subtitlesAdvancedDialog.modes[idx];
                    if (subtitlesVm.hi !== v) {
                        subtitlesVm.hi = v;
                    }
                }
            }
            FormCard.FormComboBoxDelegate {
                text: i18nc("@label subtitles filter",
                    "Foreign parts only")
                model: subtitlesAdvancedDialog.modeLabels
                currentIndex: Math.max(0,
                    subtitlesAdvancedDialog.modes.indexOf(subtitlesVm.fpo))
                onActivated: idx => {
                    const v = subtitlesAdvancedDialog.modes[idx];
                    if (subtitlesVm.fpo !== v) {
                        subtitlesVm.fpo = v;
                    }
                }
            }
        }
    }

    // ---- header: merged title + filter bar ----------------------
    header: PageHeaderBar {
        id: filterBar
        title: page.title
        pageActions: page.actions
        advancedFiltersDialog: subtitlesAdvancedDialog
        advancedFilterCount:
            (subtitlesVm.hi !== "off" ? 1 : 0)
            + (subtitlesVm.fpo !== "off" ? 1 : 0)

        // ---- Release-name filter ------------------------------
        // Bound to `subtitlesVm.release`. The view-model debounces
        // edits before re-running the search; Enter (or the Search
        // page action) bypasses the debounce and runs immediately.
        Kirigami.SearchField {
            Layout.fillWidth: true
            placeholderText: i18nc(
                "@info:placeholder subtitles release filter",
                "Release name (optional) — e.g. 1080p, BluRay, REMUX")
            text: subtitlesVm.release
            autoAccept: false
            onTextEdited: subtitlesVm.release = text
            onAccepted: {
                if (subtitlesVm.release !== text) {
                    subtitlesVm.release = text;
                }
                subtitlesVm.runSearch();
            }
        }

        // ---- Languages (multi-select) -------------------------
        FilterMenuButton {
            Layout.alignment: Qt.AlignVCenter
            axisLabel: i18nc("@action:button language picker",
                "Languages")
            icon.name: "preferences-desktop-locale"
            multiSelect: true
            options: {
                const out = [];
                const src = subtitlesVm.commonLanguages || [];
                for (let i = 0; i < src.length; ++i) {
                    const code = src[i].code !== undefined
                        ? src[i].code : "";
                    const display = src[i].display !== undefined
                        ? src[i].display : code;
                    out.push({
                        value: code,
                        label: i18nc(
                            "@item language menu, %1 display name, %2 ISO code",
                            "%1 (%2)", display, code.toUpperCase())
                    });
                }
                return out;
            }
            currentValues: subtitlesVm.languages
            onMultiActivated: list => subtitlesVm.languages = list
        }
    }

    // ---- body: chip strip + results panel -----------------------
    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // ---- active-filter chip strip ---------------------------
        Item {
            id: chipStrip

            Layout.fillWidth: true
            visible: page.subtitlesChips.length > 0
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
                            model: page.subtitlesChips

                            delegate: Kirigami.Chip {
                                required property int index
                                required property var modelData
                                text: modelData.label !== undefined
                                    ? modelData.label : ""
                                closable: true
                                checkable: false
                                Accessible.name: i18nc("@info:whatsthis",
                                    "Remove filter %1", text)
                                onRemoved: page.removeSubtitlesChip(index)
                            }
                        }
                    }
                }

                QQC2.ToolButton {
                    text: i18nc("@action:button clear every active filter",
                        "Clear all")
                    icon.name: "edit-clear-history"
                    display: QQC2.AbstractButton.TextBesideIcon
                    onClicked: subtitlesVm.resetFilters()
                }
            }
        }

        SubtitlesPanel {
            Layout.fillWidth: true
            Layout.fillHeight: true
            vm: subtitlesVm
        }
    }
}
