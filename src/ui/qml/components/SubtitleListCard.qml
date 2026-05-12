// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// One subtitle result row. Built on `BaseListCard`, which owns the
// row chrome, hover / selection styling, padding, right-click
// signalling. Surfaces the release name (fallback: file name), a
// per-row primary action button (Download / Use), and the HI / FPO
// chip rail.
//
// The footer-level primary action on `SubtitlesPanel` still works
// for the currently selected row; this card's trailing button adds
// the same affordance per row so users don't have to select-then-
// footer for every result.
BaseListCard {
    id: card

    required property int rowIndex
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

    /// View-model exposing the per-row primary action slot.
    property var vm: subtitlesVm

    onActivated: {
        if (card.vm) {
            card.vm.primaryActionForRow(card.rowIndex);
        }
    }

    onContextMenuRequested: function (pt) {
        rowMenu.popup(pt.x, pt.y);
    }

    readonly property string _displayTitle: card.release.length > 0
        ? card.release
        : (card.fileName.length > 0
            ? card.fileName
            : i18nc(
                "@info fallback when subtitle has no release name",
                "Untitled subtitle"))

    readonly property string _primaryActionText: card.active
        ? i18nc("@action:button subtitle row, currently attached",
            "Attached")
        : (card.cached
            ? i18nc("@action:button subtitle row, cached locally", "Use")
            : i18nc("@action:button subtitle row, fresh download",
                "Download"))

    readonly property string _primaryActionIcon: card.active
        ? "circle-check"
        : (card.cached ? "circle-check" : "download")

    readonly property string _stateChipText: card.active
        ? i18nc("@info subtitle row, currently attached", "Attached")
        : (card.cached
            ? i18nc("@info subtitle row, cached locally", "Cached")
            : (card.moviehashMatch
                ? i18nc("@info subtitle row, moviehash match",
                    "Hash match")
                : ""))

    readonly property string _stateChipTone: card.active || card.cached
        ? "positive"
        : (card.moviehashMatch ? "accent" : "neutral")

    readonly property string _stateChipIcon: card.active
        ? AppIcons.url("circle-check")
        : (card.cached
            ? AppIcons.url("download")
            : (card.moviehashMatch
                ? AppIcons.url("star")
                : ""))

    // Leading tile: language code + readable name. Fills the row's
    // available height; width tracks the post-layout height via
    // the tile's square aspect.
    leading: RowLeadingTile {
        Layout.fillHeight: true
        Layout.preferredHeight: Kirigami.Units.gridUnit * 4
        Layout.preferredWidth: height
        primary: card.language.toUpperCase()
        caption: card.languageName
    }

    // Body slot: title row + meta row.
    RowLayout {
        Layout.fillWidth: true
        spacing: Theme.inlineSpacing

        QQC2.Label {
            Layout.fillWidth: true
            text: card._displayTitle
            elide: Text.ElideRight
            font.weight: Font.Medium
            color: Theme.foreground
            onTruncatedChanged: card.titleTooltip
                = truncated ? text : ""
        }

        QQC2.Label {
            text: card.format
            opacity: 0.8
            visible: card.format.length > 0
        }
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: Theme.inlineSpacing

        // State chip — active / cached / moviehash signal, leading
        // the meta row so it's co-located with the rest of the
        // row's status text.
        MetaChip {
            visible: card._stateChipText.length > 0
            text: card._stateChipText
            iconSource: card._stateChipIcon
            tone: card._stateChipTone
        }

        QQC2.Label {
            text: i18ncp("@info subtitles row download count",
                "%1 download", "%1 downloads", card.downloadCount)
            opacity: 0.7
            font.pointSize: Kirigami.Theme.smallFont.pointSize
            visible: card.downloadCount > 0
        }
        QQC2.Label {
            text: "\u00b7"
            opacity: 0.5
            visible: card.downloadCount > 0 && card.rating > 0
        }
        QQC2.Label {
            text: "\u2605 %1".arg(card.rating.toFixed(1))
            opacity: 0.7
            font.pointSize: Kirigami.Theme.smallFont.pointSize
            visible: card.rating > 0
        }

        Item { Layout.fillWidth: true }

        // HI / FPO chips via MetaChip (was Kirigami.Chip).
        RowChipRail {
            Layout.alignment: Qt.AlignVCenter
            chips: {
                const list = [];
                if (card.hearingImpaired) {
                    list.push({
                        text: i18nc("@info subtitles flag", "HI"),
                        iconSource: AppIcons.url("headphones"),
                        tone: "neutral"
                    });
                }
                if (card.foreignPartsOnly) {
                    list.push({
                        text: i18nc("@info subtitles flag", "FPO"),
                        iconSource: AppIcons.url("flag"),
                        tone: "neutral"
                    });
                }
                return list;
            }
        }
    }

    // Action row: per-row primary action button, right-justified.
    trailing: RowLayout {
        spacing: Theme.inlineSpacing

        Item { Layout.fillWidth: true }

        QQC2.Button {
            text: card._primaryActionText
            icon.source: AppIcons.url(card._primaryActionIcon,
                AppIcons.controlColor(enabled, highlighted))
            icon.color: AppIcons.controlColor(enabled, highlighted)
            display: QQC2.AbstractButton.TextBesideIcon
            enabled: !card.active
                && card.vm
                && card.vm.primaryActionEnabled !== undefined
            highlighted: card.cached
            onClicked: {
                if (card.vm) {
                    card.vm.primaryActionForRow(card.rowIndex);
                }
            }
        }
    }

    QQC2.Menu {
        id: rowMenu

        QQC2.MenuItem {
            text: card._primaryActionText
            icon.source: AppIcons.url(card._primaryActionIcon)
            icon.color: AppIcons.foreground
            enabled: !card.active
            onTriggered: {
                if (card.vm) {
                    card.vm.primaryActionForRow(card.rowIndex);
                }
            }
        }
    }
}
