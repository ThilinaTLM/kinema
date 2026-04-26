// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Responsive shell for detail pages. Wide windows get two independent panes;
// narrow windows get a single vertical scroll flow. The page stack itself stays
// single-column/full-width; this is the only split view users see.
Item {
    id: scaffold

    property Component overviewComponent
    property Component workComponent
    property Item workItem: null
    readonly property bool wide: width >= Kirigami.Units.gridUnit * 64

    Loader {
        anchors.fill: parent
        sourceComponent: scaffold.wide ? wideLayout : narrowLayout
    }

    Component {
        id: wideLayout

        RowLayout {
            spacing: Kirigami.Units.largeSpacing

            QQC2.ScrollView {
                id: overviewScroll
                Layout.fillHeight: true
                Layout.preferredWidth: Math.min(
                    Math.max(scaffold.width * 0.40,
                        Kirigami.Units.gridUnit * 24),
                    Kirigami.Units.gridUnit * 36)
                Layout.maximumWidth: scaffold.width * 0.48
                clip: true
                contentWidth: availableWidth

                Loader {
                    width: overviewScroll.availableWidth
                    sourceComponent: scaffold.overviewComponent
                }
            }

            Kirigami.Separator {
                Layout.fillHeight: true
            }

            Loader {
                Layout.fillWidth: true
                Layout.fillHeight: true
                sourceComponent: scaffold.workComponent
                onLoaded: scaffold.workItem = item
            }
        }
    }

    Component {
        id: narrowLayout

        QQC2.ScrollView {
            id: narrowScroll
            clip: true
            contentWidth: availableWidth

            ColumnLayout {
                width: narrowScroll.availableWidth
                spacing: Kirigami.Units.largeSpacing * 2

                Loader {
                    Layout.fillWidth: true
                    sourceComponent: scaffold.overviewComponent
                }

                Loader {
                    Layout.fillWidth: true
                    Layout.preferredHeight: Math.max(
                        scaffold.height * 0.72,
                        Kirigami.Units.gridUnit * 28)
                    sourceComponent: scaffold.workComponent
                    onLoaded: scaffold.workItem = item
                }
            }
        }
    }
}
