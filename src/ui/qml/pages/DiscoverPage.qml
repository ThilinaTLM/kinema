// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Discover home: hero `Continue Watching` carousel followed by a
// stack of TMDB-backed content rails (Trending, Popular Series,
// Now Playing, On The Air, Top Rated Movies, Top Rated Series).
//
// Empty / not-configured / auth-failed: a `Kirigami.PlaceholderMessage`
// fills the page with an "Open Settings" call-to-action — same hook
// the legacy widget DiscoverPage exposed.
//
// Backed by `discoverVm` and `continueWatchingVm` context properties
// installed on the engine by `MainController::exposeContextProperties`.
Kirigami.ScrollablePage {
    id: page

    title: i18nc("@title:window", "Discover")
    objectName: "discover"

    leftPadding: 0
    rightPadding: 0
    topPadding: 0
    // Bottom padding lives on the inner column so the scroll
    // content keeps clean spacing without leaving an empty band
    // when the placeholder takes over.
    bottomPadding: 0

    actions: [
        Kirigami.Action {
            icon.name: "view-refresh"
            text: i18nc("@action:button", "Refresh")
            displayHint: Kirigami.DisplayHint.IconOnly
            shortcut: StandardKey.Refresh
            onTriggered: discoverVm.refresh()
        }
    ]

    // ---- Empty / not-configured / auth-failed -----------------------
    // Owns the entire page surface when active; the rails below stay
    // hidden so we don't paint two competing states.
    Kirigami.PlaceholderMessage {
        id: placeholder
        anchors.centerIn: parent
        width: Math.min(parent.width - Kirigami.Units.gridUnit * 4,
            Kirigami.Units.gridUnit * 28)
        visible: !discoverVm.tmdbConfigured || discoverVm.authFailed

        icon.name: discoverVm.authFailed ? "dialog-warning" : "configure"
        text: discoverVm.placeholderTitle
        explanation: discoverVm.placeholderBody

        helpfulAction: Kirigami.Action {
            icon.name: "settings-configure"
            text: i18nc("@action:button", "Open settings…")
            onTriggered: mainController.requestSettings()
        }
    }

    // ---- Rails ----------------------------------------------------
    ColumnLayout {
        id: stack
        width: page.width
        spacing: Kirigami.Units.largeSpacing * 2
        visible: discoverVm.tmdbConfigured && !discoverVm.authFailed

        // Top padding so the first rail isn't crammed against the
        // page header.
        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: Kirigami.Units.smallSpacing
        }

        // Continue Watching hero. Hidden when history is empty so
        // the rest of the page reflows without an empty band.
        ContinueWatchingRail {
            Layout.fillWidth: true
            visible: !continueWatchingVm.empty
        }

        // TMDB rails. The Repeater iterates over `discoverVm.sections`
        // (a `QList<QObject*>` of `DiscoverSectionModel*`s) and
        // forwards activation back through the VM with the section
        // index so it can resolve the right item.
        Repeater {
            model: discoverVm.sections

            delegate: ContentRail {
                required property var modelData
                required property int index

                Layout.fillWidth: true
                sectionModel: modelData
                onItemActivated: function (row) {
                    discoverVm.activateItem(index, row);
                }
                onShowAllRequested: discoverVm.browseSection(index)
            }
        }

        // Bottom spacer so the last rail keeps its breathing room.
        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: Kirigami.Units.gridUnit
        }
    }
}
