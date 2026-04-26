// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Phase 02 shell. Owns:
//
//  * primary navigation between Discover / Search / Browse
//    placeholder pages via `Kirigami.PageRow`
//  * drawer footer with Settings / About / Quit actions
//  * application-wide keyboard shortcuts (Quit, Preferences,
//    Find, Help, Esc-pop, Alt+M drawer)
//  * close-to-tray decision routed through `mainController`
//  * passive notifications surfaced from the C++ side
//
// Real per-page content (Discover hero / sections, Search field,
// Browse filter overlay, detail views) lands in phases 03–06.
Kirigami.ApplicationWindow {
    id: root

    title: i18nc("@title:window", "Kinema")

    width: Kirigami.Units.gridUnit * 60
    height: Kirigami.Units.gridUnit * 40
    minimumWidth: Kirigami.Units.gridUnit * 30
    minimumHeight: Kirigami.Units.gridUnit * 24

    // ---- close-to-tray --------------------------------------------------
    // The C++ side owns the decision matrix (reallyQuit /
    // closeToTray pref / tray availability / first-time toast).
    // We flip `close.accepted` to false when the controller has
    // taken responsibility for hiding the window itself.
    onClosing: function (close) {
        if (!mainController.handleWindowCloseRequested()) {
            close.accepted = false;
        }
    }

    // ---- application-level shortcuts -----------------------------------
    // Replaces the KStandardAction wiring in MainWindow. Each
    // shortcut routes through `mainController` so the C++ side
    // decides what "Settings" or "About" actually means at the
    // current phase (placeholder push now, real page in phase 06).
    Shortcut {
        sequences: [ StandardKey.Quit ]
        context: Qt.ApplicationShortcut
        onActivated: mainController.requestQuit()
    }
    Shortcut {
        sequences: [ StandardKey.Preferences ]
        context: Qt.ApplicationShortcut
        onActivated: mainController.requestSettings()
    }
    Shortcut {
        sequences: [ StandardKey.Find ]
        context: Qt.ApplicationShortcut
        onActivated: mainController.requestFocusSearch()
    }
    Shortcut {
        sequence: "F1"
        context: Qt.ApplicationShortcut
        onActivated: mainController.requestAbout()
    }
    Shortcut {
        sequence: "Esc"
        context: Qt.WindowShortcut
        onActivated: {
            if (root.pageStack.depth > 1) {
                root.pageStack.pop();
            }
        }
    }
    Shortcut {
        sequence: "Alt+M"
        context: Qt.ApplicationShortcut
        onActivated: drawer.collapsed = !drawer.collapsed
    }

    // ---- global drawer --------------------------------------------------
    // Banner = app icon + name. Top `actions` list = primary
    // navigation. Bottom inline action bar = app-level entries
    // (Settings · About · Quit) — Kirigami's drawer renders any
    // visible child below the actions list.
    globalDrawer: Kirigami.GlobalDrawer {
        id: drawer
        title: i18n("Kinema")
        titleIcon: "dev.tlmtech.kinema"
        modal: false
        collapsible: true

        actions: [
            Kirigami.Action {
                icon.name: "go-home"
                text: i18nc("@action drawer entry", "Discover")
                checkable: true
                checked: root.currentNavKey === "discover"
                onTriggered: root.showPage("discover")
            },
            Kirigami.Action {
                icon.name: "search"
                text: i18nc("@action drawer entry", "Search")
                checkable: true
                checked: root.currentNavKey === "search"
                onTriggered: root.showPage("search")
            },
            Kirigami.Action {
                icon.name: "view-list-icons"
                text: i18nc("@action drawer entry", "Browse")
                checkable: true
                checked: root.currentNavKey === "browse"
                onTriggered: root.showPage("browse")
            }
        ]

        // Footer row: rendered inline below the nav list. Kirigami
        // hides icon labels in collapsed mode; tooltips fall back
        // to the localized text. Settings + About push pages onto
        // the stack via `mainController`.
        Kirigami.ActionToolBar {
            Layout.fillWidth: true
            display: QQC2.AbstractButton.TextBesideIcon
            alignment: Qt.AlignLeft
            actions: [
                Kirigami.Action {
                    icon.name: "settings-configure"
                    text: i18nc("@action drawer footer", "Settings")
                    onTriggered: mainController.requestSettings()
                },
                Kirigami.Action {
                    icon.name: "help-about"
                    text: i18nc("@action drawer footer", "About")
                    onTriggered: mainController.requestAbout()
                },
                Kirigami.Action {
                    icon.name: "application-exit"
                    text: i18nc("@action drawer footer", "Quit")
                    onTriggered: mainController.requestQuit()
                }
            ]
        }
    }

    // ---- pageStack and navigation --------------------------------------
    // Single-column push-style. Kirigami collapses to overlay on
    // narrow widths automatically. The drawer's checked state and
    // the `showPage` helper rely on the top-of-stack page's
    // `objectName` which is set on every placeholder page.
    pageStack.initialPage: discoverComp

    readonly property string currentNavKey: pageStack.currentItem
        ? (pageStack.currentItem.objectName || "")
        : ""

    function showPage(key) {
        if (currentNavKey === key) {
            // Re-clicking the active drawer entry should stay put;
            // the checkable state already pinned it.
            return;
        }
        switch (key) {
        case "discover":
            pageStack.replace(discoverComp);
            break;
        case "search":
            pageStack.replace(searchComp);
            break;
        case "browse":
            pageStack.replace(browseComp);
            break;
        }
    }

    // Page factories. Re-instantiated on each navigation so a
    // returning visit lands on a fresh state. Phases 03–06 swap
    // these out for the real Kirigami pages backed by view-models.
    // Phase 03 swaps the Discover placeholder for the real page.
    // Search / Browse stay on placeholders until phase 04.
    Component { id: discoverComp; DiscoverPage           { } }
    Component { id: searchComp;   SearchPagePlaceholder  { objectName: "search" } }
    Component { id: browseComp;   BrowsePagePlaceholder  { objectName: "browse" } }

    // About / Settings — pushed on top of the current nav stack.
    // The Settings stub is a placeholder page until phase 06
    // ships `Kirigami.CategorizedSettings` against the real VMs.
    Component {
        id: aboutComp
        KAboutPage { }
    }
    Component {
        id: settingsComp
        Kirigami.Page {
            title: i18nc("@title:window", "Settings")
            Kirigami.PlaceholderMessage {
                anchors.centerIn: parent
                width: parent.width - Kirigami.Units.largeSpacing * 4
                icon.name: "settings-configure"
                text: i18nc("@info placeholder",
                    "Settings are coming back in a later phase of the "
                    + "Kirigami migration.")
                explanation: i18nc("@info placeholder",
                    "Existing tokens and preferences continue to apply; "
                    + "they just can't be edited from the app right now.")
            }
        }
    }

    // ---- C++-driven flows ----------------------------------------------
    Connections {
        target: mainController

        function onShowSettingsRequested() {
            // Single Settings instance on top of the stack; if
            // already visible, re-pushing is a no-op so the back
            // button still pops to the previous nav page.
            if (root.pageStack.currentItem
                && root.pageStack.currentItem.objectName === "settings") {
                return;
            }
            root.pageStack.push(settingsComp, { objectName: "settings" });
        }
        function onShowAboutRequested() {
            if (root.pageStack.currentItem
                && root.pageStack.currentItem.objectName === "about") {
                return;
            }
            root.pageStack.push(aboutComp, { objectName: "about" });
        }
        function onFocusSearchRequested() {
            // Phase 02: there is no Search field yet, so the
            // Ctrl+F shortcut just navigates to the Search page.
            // Phase 04 wires `focusSearchField()` on the real
            // SearchPage and prefers it over re-pushing.
            const top = root.pageStack.currentItem;
            if (top && typeof top.focusSearchField === "function") {
                top.focusSearchField();
                return;
            }
            root.showPage("search");
        }
        function onPassiveMessage(text, durationMs) {
            root.showPassiveNotification(text, durationMs);
        }
    }
}
