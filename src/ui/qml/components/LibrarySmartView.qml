// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Smart view of the Library page. Vertical stack of horizontal
// rails: Up Next, Airing Soon, Recently Added. Each rail
// self-hides when its model is empty.
//
// Implementation: plain Flickable for the vertical scroll, with a
// `ColumnLayout` of rails as its content. Each rail's inner
// `ListView` flicks horizontally; vertical and horizontal flick
// directions don't conflict.
//
// Avoids `QQC2.ScrollView` here because its `availableWidth`
// binding doesn't settle correctly when the scroll view is itself
// a child of a `StackLayout` \u2014 the inner Flickable ends up
// 0-wide and nothing renders. Plain Flickable + explicit
// `width: parent.width` on the column is reliable.
//
// No filter chrome lives here \u2014 filters are a List-view concern.
Flickable {
    id: view

    /// Caller wires this to `libraryVm`. Decoupled rather than
    /// referencing the context property directly so the component
    /// can be tested or reused with a stub VM. Named `vm` (not
    /// `libraryVm`) so the caller's `vm: libraryVm` binding doesn't
    /// self-reference: a property named `libraryVm` on this object
    /// would shadow the context property of the same name when QML
    /// resolves the right-hand side against the scope object.
    property var vm

    contentWidth: width
    contentHeight: rails.implicitHeight
    boundsBehavior: Flickable.StopAtBounds
    clip: true

    QQC2.ScrollBar.vertical: QQC2.ScrollBar { policy: QQC2.ScrollBar.AsNeeded }

    ColumnLayout {
        id: rails
        width: view.width
        spacing: Theme.sectionSpacing

        // Top breathing room consistent with Discover.
        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: Theme.sectionSpacing
        }

        LibraryRail {
            Layout.fillWidth: true
            title: i18nc("@title library rail", "Up Next")
            artworkShape: "thumbnail"
            model: view.vm ? view.vm.upNextModel : null
            visible: !!(view.vm
                && view.vm.upNextModel
                && view.vm.upNextModel.count > 0)
            onItemActivated: row => view.vm
                && view.vm.activateRail("upNext", row)
        }
        LibraryRail {
            Layout.fillWidth: true
            title: i18nc("@title library rail", "Airing Soon")
            artworkShape: "thumbnail"
            model: view.vm ? view.vm.airingSoonModel : null
            visible: !!(view.vm
                && view.vm.airingSoonModel
                && view.vm.airingSoonModel.count > 0)
            onItemActivated: row => view.vm
                && view.vm.activateRail("airingSoon", row)
        }
        LibraryRail {
            Layout.fillWidth: true
            title: i18nc("@title library rail", "Recently Added")
            artworkShape: "poster"
            model: view.vm ? view.vm.recentlyAddedModel : null
            visible: !!(view.vm
                && view.vm.recentlyAddedModel
                && view.vm.recentlyAddedModel.count > 0)
            onItemActivated: row => view.vm
                && view.vm.activateRail("recentlyAdded", row)
        }

        // Empty smart-view fallback when the user has saved
        // titles but no rail has anything to show right now.
        Kirigami.PlaceholderMessage {
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: Theme.sectionSpacing
            Layout.preferredWidth: Math.min(view.width
                    - Theme.pageWideMargin * 2,
                Theme.placeholderMaxWidth)
            visible: !!(view.vm
                && (!view.vm.upNextModel
                    || view.vm.upNextModel.count === 0)
                && (!view.vm.airingSoonModel
                    || view.vm.airingSoonModel.count === 0)
                && (!view.vm.recentlyAddedModel
                    || view.vm.recentlyAddedModel.count === 0))
            icon.source: AppIcons.url("sparkles")
            icon.color: AppIcons.foreground
            text: i18nc("@info placeholder",
                "Nothing to surface right now")
            explanation: i18nc("@info placeholder",
                "Add some movies or shows to your Library to "
                + "see what's up next, airing soon, or recently "
                + "added here.")
        }

        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: Theme.pageBottomSpacing
        }
    }
}
