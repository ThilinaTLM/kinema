// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Subtitles search/download page.
//
// Chrome:
//
//   * `header:` is a `QQC2.ToolBar` (Header colorSet) pairing a
//     `Kirigami.SearchField` (bound to `subtitlesVm.release` — the
//     optional "Release name" filter) with a `Kirigami.ActionToolBar`
//     of filter actions: Languages ▾ (multi-select Menu), HI ▾, FPO ▾
//     (pick-one sub-menus).
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

            // ---- Languages / HI / FPO -----------------------------
            Kirigami.ActionToolBar {
                id: filterToolBar
                Layout.alignment: Qt.AlignVCenter
                alignment: Qt.AlignRight
                flat: true

                actions: [
                    // ---- Languages ▾ (multi-select Menu) -----------
                    Kirigami.Action {
                        id: languagesAction
                        text: {
                            const langs = subtitlesVm.languages || [];
                            if (langs.length === 0) {
                                return i18nc("@action:button language picker, no selection",
                                    "Languages: any");
                            }
                            if (langs.length <= 3) {
                                return i18nc("@action:button language picker, %1 is a comma-joined list of ISO codes",
                                    "Languages: %1",
                                    langs.map(c => c.toUpperCase()).join(", "));
                            }
                            return i18ncp("@action:button language picker, count summary",
                                "Languages: %1", "Languages: %1",
                                langs.length);
                        }
                        icon.name: "preferences-desktop-locale"

                        displayComponent: QQC2.ToolButton {
                            id: langsBtn
                            // Match Kirigami.ActionToolBar's native button
                            // styling so this row aligns with the rest of
                            // the actions (flat, same display mode).
                            flat: true
                            display: filterToolBar.display
                            text: languagesAction.text
                            // `Kirigami.Action.icon` is a grouped property;
                            // reading `.name` from outside the action scope
                            // doesn't propagate reliably to the displayed
                            // ToolButton, so hard-code it here. The wrapper
                            // action above keeps `icon.name` set so the
                            // overflow menu renders the icon too.
                            icon.name: "preferences-desktop-locale"
                            checkable: true
                            checked: langsMenu.opened
                            onClicked: langsMenu.opened
                                ? langsMenu.close()
                                : langsMenu.open()

                            QQC2.Menu {
                                id: langsMenu
                                y: langsBtn.height
                                implicitWidth: Math.max(langsBtn.width,
                                    Kirigami.Units.gridUnit * 14)

                                QQC2.MenuItem {
                                    text: i18nc("@action language picker, drop selection",
                                        "Clear (any language)")
                                    icon.name: "edit-clear-history"
                                    enabled: (subtitlesVm.languages || []).length > 0
                                    onTriggered: subtitlesVm.languages = []
                                }
                                QQC2.MenuSeparator { }

                                Repeater {
                                    model: subtitlesVm.commonLanguages
                                    delegate: QQC2.MenuItem {
                                        required property var modelData
                                        text: `${modelData.display} (${modelData.code.toUpperCase()})`
                                        checkable: true
                                        checked: (subtitlesVm.languages || [])
                                            .indexOf(modelData.code) >= 0
                                        onTriggered: {
                                            const code = modelData.code;
                                            const next = (subtitlesVm.languages || []).slice();
                                            const at = next.indexOf(code);
                                            if (at >= 0) {
                                                next.splice(at, 1);
                                            } else {
                                                next.push(code);
                                            }
                                            subtitlesVm.languages = next;
                                        }
                                    }
                                }
                            }
                        }
                    },

                    // ---- HI ▾ (pick-one sub-menu) ------------------
                    Kirigami.Action {
                        text: i18nc("@action:button subtitles filter",
                            "Hearing impaired")
                        icon.name: "audio-volume-muted"
                        children: [
                            Kirigami.Action {
                                text: i18nc("@option:radio subtitle filter mode", "Off")
                                checkable: true
                                checked: subtitlesVm.hi === "off"
                                onTriggered: if (subtitlesVm.hi !== "off") {
                                    subtitlesVm.hi = "off";
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@option:radio subtitle filter mode", "Include")
                                checkable: true
                                checked: subtitlesVm.hi === "include"
                                onTriggered: if (subtitlesVm.hi !== "include") {
                                    subtitlesVm.hi = "include";
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@option:radio subtitle filter mode", "Only")
                                checkable: true
                                checked: subtitlesVm.hi === "only"
                                onTriggered: if (subtitlesVm.hi !== "only") {
                                    subtitlesVm.hi = "only";
                                }
                            }
                        ]
                    },

                    // ---- FPO ▾ (pick-one sub-menu) -----------------
                    Kirigami.Action {
                        text: i18nc("@action:button subtitles filter",
                            "Foreign parts only")
                        icon.name: "format-text-direction-ltr"
                        children: [
                            Kirigami.Action {
                                text: i18nc("@option:radio subtitle filter mode", "Off")
                                checkable: true
                                checked: subtitlesVm.fpo === "off"
                                onTriggered: if (subtitlesVm.fpo !== "off") {
                                    subtitlesVm.fpo = "off";
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@option:radio subtitle filter mode", "Include")
                                checkable: true
                                checked: subtitlesVm.fpo === "include"
                                onTriggered: if (subtitlesVm.fpo !== "include") {
                                    subtitlesVm.fpo = "include";
                                }
                            },
                            Kirigami.Action {
                                text: i18nc("@option:radio subtitle filter mode", "Only")
                                checkable: true
                                checked: subtitlesVm.fpo === "only"
                                onTriggered: if (subtitlesVm.fpo !== "only") {
                                    subtitlesVm.fpo = "only";
                                }
                            }
                        ]
                    }
                ]
            }
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
