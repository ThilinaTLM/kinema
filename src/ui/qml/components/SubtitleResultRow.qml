// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// One row in the subtitles list. Displays the release name, file
// name fallback, language, format, downloads, rating, and badges
// for HI / FPO / cached / active / moviehash-match. Clicking the
// row selects it; double-clicking triggers the page's primary
// action (Download / Use / Re-attach).
QQC2.ItemDelegate {
    id: row

    required property string release
    required property string fileName
    required property string language
    required property string languageName
    required property string format
    required property int downloadCount
    required property real rating
    required property bool hearingImpaired
    required property bool foreignPartsOnly
    required property bool moviehashMatch
    required property bool cached
    required property bool active

    property bool selected: false

    signal activated()

    width: ListView.view ? ListView.view.width : implicitWidth
    highlighted: selected
    onDoubleClicked: row.activated()

    contentItem: ColumnLayout {
        spacing: Theme.inlineSpacing

        RowLayout {
            spacing: Theme.inlineSpacing
            Layout.fillWidth: true

            Kirigami.Icon {
                visible: row.active || row.cached || row.moviehashMatch
                source: row.active
                    ? "emblem-checked"
                    : (row.cached ? "emblem-downloads" : "starred-symbolic")
                Layout.preferredWidth: Kirigami.Units.iconSizes.small
                Layout.preferredHeight: Kirigami.Units.iconSizes.small
            }

            QQC2.Label {
                text: row.release.length > 0
                    ? row.release
                    : (row.fileName.length > 0
                        ? row.fileName
                        : i18nc("@info fallback when subtitle has no release name",
                            "Untitled subtitle"))
                Layout.fillWidth: true
                elide: Text.ElideRight
                font.weight: Font.Medium
            }

            QQC2.Label {
                text: row.language.toUpperCase()
                opacity: 0.8
            }
            QQC2.Label {
                text: row.format
                opacity: 0.8
                visible: row.format.length > 0
            }
        }

        RowLayout {
            spacing: Theme.inlineSpacing
            Layout.fillWidth: true

            QQC2.Label {
                text: row.languageName
                opacity: 0.7
                font.pointSize: Kirigami.Theme.smallFont.pointSize
            }
            QQC2.Label {
                text: "·"
                opacity: 0.5
                visible: row.downloadCount > 0
            }
            QQC2.Label {
                text: i18ncp("@info subtitles row download count",
                    "%1 download", "%1 downloads", row.downloadCount)
                opacity: 0.7
                font.pointSize: Kirigami.Theme.smallFont.pointSize
                visible: row.downloadCount > 0
            }
            QQC2.Label {
                text: "·"
                opacity: 0.5
                visible: row.rating > 0
            }
            QQC2.Label {
                text: "★ %1".arg(row.rating.toFixed(1))
                opacity: 0.7
                font.pointSize: Kirigami.Theme.smallFont.pointSize
                visible: row.rating > 0
            }

            Item { Layout.fillWidth: true }

            // Inline badges. Use the same icon names as the widget
            // delegate so Plasma icon themes light up consistently.
            Kirigami.Chip {
                visible: row.hearingImpaired
                checkable: false
                closable: false
                icon.name: "audio-headset-symbolic"
                text: i18nc("@info subtitles flag", "HI")
            }
            Kirigami.Chip {
                visible: row.foreignPartsOnly
                checkable: false
                closable: false
                icon.name: "flag-symbolic"
                text: i18nc("@info subtitles flag", "FPO")
            }
        }
    }
}
