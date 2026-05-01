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
            icon.source: AppIcons.url("search")
            icon.color: AppIcons.foreground
            text: i18nc("@action:button subtitles run search", "Search")
            displayHint: Kirigami.DisplayHint.IconOnly
            enabled: subtitlesVm.state !== "notconfigured"
            onTriggered: subtitlesVm.runSearch()
        },
        Kirigami.Action {
            icon.source: AppIcons.url("settings")
            icon.color: AppIcons.foreground
            text: i18nc("@action subtitles page header, open settings",
                "Open settings…")
            onTriggered: subtitlesVm.openSettings()
        }
    ]

    // ---- active-filter chips (derived from VM filter properties) -
    // Removed: active filters are now visible via the FilterMenuButton
    // labels and the "More filters (N)" count, with a "Reset" action
    // inside the advanced dialog.

    // ---- advanced filters dialog --------------------------------
    Kirigami.Dialog {
        id: subtitlesAdvancedDialog
        title: i18nc("@title:dialog subtitles advanced filters",
            "More filters")
        preferredWidth: Kirigami.Units.gridUnit * 26

        footer: QQC2.DialogButtonBox {
            QQC2.Button {
                text: i18nc("@action:button reset all subtitle filters",
                    "Reset all filters")
                icon.source: AppIcons.url("eraser")
                icon.color: AppIcons.foreground
                visible: subtitlesVm.languages.length > 0
                    || subtitlesVm.hi !== "off"
                    || subtitlesVm.fpo !== "off"
                QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.ActionRole
                onClicked: {
                    subtitlesVm.resetFilters();
                    subtitlesAdvancedDialog.close();
                }
            }
            QQC2.Button {
                text: i18nc("@action:button close dialog", "Close")
                QQC2.DialogButtonBox.buttonRole: QQC2.DialogButtonBox.AcceptRole
                onClicked: subtitlesAdvancedDialog.close()
            }
        }

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
                text: i18nc("@label subtitles filter", "Foreign parts only")
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
            icon.source: AppIcons.url("languages")
            multiSelect: true
            active: subtitlesVm.languages.length > 0
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

    // ---- body: results panel -----------------------------------
    SubtitlesPanel {
        anchors.fill: parent
        vm: subtitlesVm
    }
}
