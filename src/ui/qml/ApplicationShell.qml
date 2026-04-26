// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Phase 01 shell: a Kirigami.ApplicationWindow with a global drawer
// and three placeholder pages. Real pages, navigation, actions,
// shortcuts, and tray glue land in phases 02+.
Kirigami.ApplicationWindow {
    id: root

    title: i18nc("@title:window", "Kinema")

    width: Kirigami.Units.gridUnit * 60
    height: Kirigami.Units.gridUnit * 40
    minimumWidth: Kirigami.Units.gridUnit * 30
    minimumHeight: Kirigami.Units.gridUnit * 24

    // The drawer is the Kirigami equivalent of the old hamburger
    // menu and primary navigation toolbar. Phase 02 fills out the
    // footer (Settings / About / Quit).
    globalDrawer: Kirigami.GlobalDrawer {
        id: drawer
        title: "Kinema"
        titleIcon: "dev.tlmtech.kinema"
        modal: false
        collapsible: true

        actions: [
            Kirigami.Action {
                icon.name: "go-home"
                text: i18nc("@action drawer entry", "Discover")
                checkable: true
                checked: root.pageStack.currentItem
                    && root.pageStack.currentItem.objectName === "discover"
                onTriggered: root.showPage("discover")
            },
            Kirigami.Action {
                icon.name: "search"
                text: i18nc("@action drawer entry", "Search")
                checkable: true
                checked: root.pageStack.currentItem
                    && root.pageStack.currentItem.objectName === "search"
                onTriggered: root.showPage("search")
            },
            Kirigami.Action {
                icon.name: "view-list-icons"
                text: i18nc("@action drawer entry", "Browse")
                checkable: true
                checked: root.pageStack.currentItem
                    && root.pageStack.currentItem.objectName === "browse"
                onTriggered: root.showPage("browse")
            }
        ]
    }

    // Page components — created on demand. We don't keep references
    // because PageRow.replace() takes the Component itself.
    Component {
        id: discoverComp
        DiscoverPagePlaceholder { objectName: "discover" }
    }
    Component {
        id: searchComp
        SearchPagePlaceholder { objectName: "search" }
    }
    Component {
        id: browseComp
        BrowsePagePlaceholder { objectName: "browse" }
    }

    pageStack.initialPage: discoverComp

    // Centralised navigation entry point. Phase 02 hooks up
    // shortcuts (Ctrl+F → search, Alt+Home → discover, …) that
    // route through this same call.
    function showPage(key) {
        const current = pageStack.currentItem
            ? pageStack.currentItem.objectName : ""
        if (current === key) {
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
}
