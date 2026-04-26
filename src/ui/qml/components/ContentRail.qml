// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Generic horizontal content row used by Discover. Header on top
// (title + Show all action), then a horizontally-scrolling
// `ListView` of `PosterCard` delegates fed by a
// `DiscoverSectionModel`. A small inline placeholder takes over the
// rail when the section is loading / empty / errored so the parent
// `Repeater` doesn't need to special-case state.
ColumnLayout {
    id: rail

    /// Required: a `DiscoverSectionModel*` from the C++ side. Carries
    /// items + title + state.
    property var sectionModel

    signal itemActivated(int row)
    signal showAllRequested()

    spacing: Kirigami.Units.smallSpacing

    // ---- Header ---------------------------------------------------
    RowLayout {
        Layout.fillWidth: true
        Layout.leftMargin: Kirigami.Units.gridUnit
        Layout.rightMargin: Kirigami.Units.gridUnit

        Kirigami.Heading {
            level: 3
            text: rail.sectionModel ? rail.sectionModel.title : ""
            Layout.fillWidth: true
            elide: Text.ElideRight
        }

        QQC2.ToolButton {
            text: i18nc("@action:button", "Show all")
            icon.name: "go-next"
            display: QQC2.AbstractButton.TextBesideIcon
            visible: rail.sectionModel
                && rail.sectionModel.state === DiscoverSectionModel.Ready
            onClicked: rail.showAllRequested()
        }
    }

    // ---- Body: state-specific surface -----------------------------
    // Each state owns a single child. We use Loader so unused states
    // don't pay layout cost.
    Loader {
        id: body
        Layout.fillWidth: true
        Layout.preferredHeight: Math.round(Theme.posterMin * 1.5)
            + Kirigami.Units.gridUnit * 3   // poster + meta block

        active: rail.sectionModel !== undefined && rail.sectionModel !== null

        sourceComponent: {
            if (!rail.sectionModel) return null;
            switch (rail.sectionModel.state) {
            case DiscoverSectionModel.Loading:
                return loadingComp;
            case DiscoverSectionModel.Empty:
            case DiscoverSectionModel.Idle:
                return emptyComp;
            case DiscoverSectionModel.Error:
                return errorComp;
            case DiscoverSectionModel.Ready:
            default:
                return listComp;
            }
        }
    }

    // ---- Components -----------------------------------------------
    Component {
        id: listComp

        ListView {
            id: list
            orientation: ListView.Horizontal
            clip: true
            spacing: Kirigami.Units.largeSpacing
            leftMargin: Kirigami.Units.gridUnit
            rightMargin: Kirigami.Units.gridUnit
            boundsBehavior: Flickable.StopAtBounds
            // Cache one viewport's worth of delegates either side
            // so flicking doesn't drop posters in/out at the edges.
            cacheBuffer: width
            model: rail.sectionModel

            delegate: PosterCard {
                width: Theme.posterMin
                height: list.height
                posterUrl: model.posterUrl
                title:     model.title
                subtitle:  model.year > 0 ? String(model.year) : ""
                rating:    model.voteAverage !== undefined
                    ? model.voteAverage : -1
                onClicked: rail.itemActivated(index)
            }
        }
    }

    Component {
        id: loadingComp
        Item {
            QQC2.BusyIndicator {
                anchors.centerIn: parent
                running: true
            }
        }
    }

    Component {
        id: emptyComp
        Item {
            QQC2.Label {
                anchors.centerIn: parent
                text: i18nc("@info discover empty rail",
                    "Nothing here right now.")
                color: Theme.disabled
            }
        }
    }

    Component {
        id: errorComp
        ColumnLayout {
            anchors.centerIn: parent
            spacing: Kirigami.Units.smallSpacing

            Kirigami.Icon {
                Layout.alignment: Qt.AlignHCenter
                source: "dialog-warning"
                width: Kirigami.Units.iconSizes.medium
                height: width
            }
            QQC2.Label {
                Layout.alignment: Qt.AlignHCenter
                text: rail.sectionModel
                    ? rail.sectionModel.errorMessage
                    : ""
                color: Theme.disabled
                wrapMode: Text.Wrap
                horizontalAlignment: Text.AlignHCenter
                Layout.maximumWidth: rail.width - Kirigami.Units.gridUnit * 4
            }
        }
    }
}
