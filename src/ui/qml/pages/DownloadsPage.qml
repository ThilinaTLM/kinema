// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

Kirigami.Page {
    id: page

    title: i18nc("@title:window", "Downloads")
    objectName: "downloads"

    leftPadding: 0
    rightPadding: 0
    topPadding: 0
    bottomPadding: 0

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // Header strip with cache stats.
        Kirigami.AbstractApplicationHeader {
            Layout.fillWidth: true
            Layout.preferredHeight: Kirigami.Units.gridUnit * 3
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: Kirigami.Units.largeSpacing
                anchors.rightMargin: Kirigami.Units.largeSpacing
                spacing: Kirigami.Units.largeSpacing

                Kirigami.Heading {
                    level: 3
                    text: i18nc("@title", "Downloads")
                }

                QQC2.Label {
                    color: Kirigami.Theme.disabledTextColor
                    text: {
                        const a = downloadsVm.activeCount;
                        const c = downloadsVm.completedCount;
                        const f = downloadsVm.failedCount;
                        return i18nc("@info downloads counts",
                            "%1 active · %2 completed · %3 failed",
                            a, c, f);
                    }
                }

                Item { Layout.fillWidth: true }

                QQC2.Label {
                    color: Kirigami.Theme.disabledTextColor
                    text: i18nc("@info cache totals",
                        "Cache: %1 (%2 evictable)",
                        downloadsVm.cacheSizeText,
                        downloadsVm.ephemeralSizeText)
                }
            }
        }

        // Empty state.
        Kirigami.PlaceholderMessage {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: downloadsVm.items.count === 0
            icon.name: "download"
            text: i18nc("@info:placeholder",
                "No downloads yet")
            explanation: i18nc("@info:placeholder",
                "Stream or pin a release from a detail page to see "
                + "it here. Pinned items stay until you remove them; "
                + "everything else can be evicted automatically.")
        }

        // List of download rows.
        QQC2.ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: downloadsVm.items.count > 0
            clip: true

            ListView {
                id: list
                model: downloadsVm.items
                spacing: Kirigami.Units.smallSpacing

                delegate: DownloadRow {
                    width: list.width
                }
            }
        }
    }
}
