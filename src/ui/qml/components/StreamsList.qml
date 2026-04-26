// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Streams region for a detail page: a header row (count + cached-on-RD
// toggle), then a state-switched body that renders one of the five
// `StreamsListModel.State` cases (Loading / Ready / Empty / Error /
// Unreleased).
//
// Sort UX lives on the parent page (a `Kirigami.Action` that opens
// `StreamSortMenu`); the list itself is unaware of how the rows
// arrived in their current order.
ColumnLayout {
    id: streams

    /// View-model exposing `streams` (StreamsListModel*),
    /// `cachedOnly`, `realDebridConfigured`, and `rawStreamsCount`.
    /// Defaults to `movieDetailVm`; commit B's SeriesDetailPage
    /// rebinds it.
    property var vm: movieDetailVm

    spacing: Kirigami.Units.smallSpacing

    // ---- header row -------------------------------------------
    RowLayout {
        Layout.fillWidth: true
        Layout.leftMargin: Kirigami.Units.largeSpacing
        Layout.rightMargin: Kirigami.Units.largeSpacing
        spacing: Kirigami.Units.largeSpacing

        Kirigami.Heading {
            level: 3
            text: streams.vm.streams && streams.vm.streams.count > 0
                ? i18ncp("@title:section",
                    "%1 stream", "%1 streams",
                    streams.vm.streams.count)
                : i18nc("@title:section", "Streams")
            color: Theme.foreground
        }

        Item { Layout.fillWidth: true }

        QQC2.CheckBox {
            text: i18nc("@option:check",
                "Cached on Real-Debrid only")
            visible: streams.vm.realDebridConfigured
                && streams.vm.rawStreamsCount > 0
            checked: streams.vm.cachedOnly
            onToggled: streams.vm.cachedOnly = checked
        }
    }

    // ---- state-switched body ----------------------------------
    StackLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true

        // The model state enum order is Idle / Loading / Ready /
        // Empty / Error / Unreleased — the indices below mirror it
        // 1:1 so an extra state added later only needs the new
        // child appended in matching order.
        currentIndex: streams.vm.streams
            ? streams.vm.streams.state
            : 0

        // 0 — Idle (no title loaded yet).
        Item { }

        // 1 — Loading.
        Item {
            QQC2.BusyIndicator {
                anchors.centerIn: parent
                running: parent.visible
            }
        }

        // 2 — Ready: virtualised list of StreamCards.
        ListView {
            id: list
            model: streams.vm.streams
            clip: true
            spacing: 0
            cacheBuffer: Kirigami.Units.gridUnit * 20
            boundsBehavior: Flickable.StopAtBounds

            delegate: StreamCard {
                row: index
                releaseName: model.releaseName
                detailsText: model.detailsText
                chips: model.chips
                sizeText: model.sizeText
                seeders: model.seeders
                rdCached: model.rdCached
                rdDownload: model.rdDownload
                hasMagnet: model.hasMagnet
                hasDirectUrl: model.hasDirectUrl
                vm: streams.vm
            }
        }

        // 3 — Empty (no rows after filters / upstream returned 0).
        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: Math.min(parent.width
                    - Kirigami.Units.gridUnit * 4,
                Kirigami.Units.gridUnit * 28)
            icon.name: "edit-find"
            text: i18nc("@info placeholder", "No streams")
            explanation: streams.vm.streams
                ? streams.vm.streams.emptyExplanation : ""
        }

        // 4 — Error.
        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: Math.min(parent.width
                    - Kirigami.Units.gridUnit * 4,
                Kirigami.Units.gridUnit * 28)
            icon.name: "dialog-error"
            text: i18nc("@info placeholder", "Couldn't fetch streams")
            explanation: streams.vm.streams
                ? streams.vm.streams.errorMessage : ""
            helpfulAction: Kirigami.Action {
                icon.name: "view-refresh"
                text: i18nc("@action:button", "Retry")
                onTriggered: streams.vm.retry()
            }
        }

        // 5 — Unreleased.
        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: Math.min(parent.width
                    - Kirigami.Units.gridUnit * 4,
                Kirigami.Units.gridUnit * 28)
            icon.name: "appointment-soon"
            text: i18nc("@info placeholder",
                "Not released yet")
            explanation: (streams.vm.streams
                && streams.vm.streams.releaseDate
                && streams.vm.streams.releaseDate.toString().length > 0)
                ? i18nc("@info placeholder body. %1 is a localized date",
                    "Streams will be available from %1.",
                    Qt.formatDate(
                        streams.vm.streams.releaseDate, Qt.DefaultLocaleLongDate))
                : ""
        }
    }
}
