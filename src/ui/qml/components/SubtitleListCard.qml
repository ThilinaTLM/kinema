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
// per-row primary action button (Download / Use), and a
// dot-separated caption meta line (state · downloads · rating ·
// HI · FPO).
//
// The footer-level primary action on `SubtitlesPanel` still works
// for the currently selected row; this card's trailing button adds
// the same affordance per row so users don't have to select-then-
// footer for every result.
//
// The meta row previously mixed a `MetaChip` (state) and a
// `RowChipRail` (HI / FPO flags) with plain caption labels;
// the chip's vertical padding made the row taller whenever any
// chip was present and the row jittered between heights. All
// tokens are now caption-sized text (with optional inline icon)
// so the line height is identical in every state.
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

    // Map a state tone string to a palette colour for the state
    // token on the meta row. Tone moves from a pill-chip border
    // (old design) to label colour so the meta row keeps a single
    // caption-sized line height for every state.
    function _stateColor(tone) {
        switch (tone) {
        case "positive": return Theme.positive;
        case "accent":   return Theme.accent;
        default:         return Theme.disabled;
        }
    }

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

    // Meta row: dot-separated caption tokens — state (icon +
    // toned label), downloads, rating, HI (icon + label), FPO
    // (icon + label). All caption-sized text + small inline icons
    // so the line height stays identical regardless of which
    // tokens render.
    RowLayout {
        id: metaRow

        readonly property bool hasState: card._stateChipText.length > 0
        readonly property bool hasDownloads: card.downloadCount > 0
        readonly property bool hasRating: card.rating > 0
        readonly property bool hasHi: card.hearingImpaired
        readonly property bool hasFpo: card.foreignPartsOnly

        Layout.fillWidth: true
        spacing: Theme.inlineSpacing

        // State icon — active / cached / moviehash signal. Sized
        // down so it sits on the caption baseline.
        Kirigami.Icon {
            Layout.alignment: Qt.AlignVCenter
            visible: metaRow.hasState
                && card._stateChipIcon.length > 0
            Layout.preferredWidth:
                Math.round(Kirigami.Units.iconSizes.small * 0.8)
            Layout.preferredHeight: width
            source: card._stateChipIcon
            color: card._stateColor(card._stateChipTone)
        }

        // State label — tone moves from chip border to label
        // colour.
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: metaRow.hasState
            text: card._stateChipText
            font.pointSize: Theme.captionFont.pointSize
            color: card._stateColor(card._stateChipTone)
            verticalAlignment: Text.AlignVCenter
        }

        // (state) → (downloads)
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: metaRow.hasState && metaRow.hasDownloads
            text: "\u00b7"
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: metaRow.hasDownloads
            text: i18ncp("@info subtitles row download count",
                "%1 download", "%1 downloads", card.downloadCount)
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        // (state|downloads) → (rating)
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: (metaRow.hasState || metaRow.hasDownloads)
                && metaRow.hasRating
            text: "\u00b7"
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: metaRow.hasRating
            text: "\u2605 %1".arg(card.rating.toFixed(1))
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        // (state|downloads|rating) → (HI)
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: (metaRow.hasState || metaRow.hasDownloads
                || metaRow.hasRating) && metaRow.hasHi
            text: "\u00b7"
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        Kirigami.Icon {
            Layout.alignment: Qt.AlignVCenter
            visible: metaRow.hasHi
            Layout.preferredWidth:
                Math.round(Kirigami.Units.iconSizes.small * 0.8)
            Layout.preferredHeight: width
            source: AppIcons.url("headphones")
            color: Theme.disabled
        }

        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: metaRow.hasHi
            text: i18nc("@info subtitles flag", "HI")
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        // (state|downloads|rating|HI) → (FPO)
        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: (metaRow.hasState || metaRow.hasDownloads
                || metaRow.hasRating || metaRow.hasHi)
                && metaRow.hasFpo
            text: "\u00b7"
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        Kirigami.Icon {
            Layout.alignment: Qt.AlignVCenter
            visible: metaRow.hasFpo
            Layout.preferredWidth:
                Math.round(Kirigami.Units.iconSizes.small * 0.8)
            Layout.preferredHeight: width
            source: AppIcons.url("flag")
            color: Theme.disabled
        }

        QQC2.Label {
            Layout.alignment: Qt.AlignVCenter
            visible: metaRow.hasFpo
            text: i18nc("@info subtitles flag", "FPO")
            font.pointSize: Theme.captionFont.pointSize
            color: Theme.disabled
            verticalAlignment: Text.AlignVCenter
        }

        // Trailing fill keeps the row packed flush left when the
        // body stretches it to the card width.
        Item { Layout.fillWidth: true }
    }

    // Action row: per-row primary action button, left-aligned.
    trailing: RowLayout {
        spacing: Theme.inlineSpacing

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

    KinemaMenu {
        id: rowMenu

        KinemaMenuItem {
            iconName: card._primaryActionIcon
            label: card._primaryActionText
            enabled: !card.active
            onTriggered: {
                if (card.vm) {
                    card.vm.primaryActionForRow(card.rowIndex);
                }
            }
        }
        QQC2.MenuSeparator { }
        // TODO: re-add "Open Source URL" / "Copy Source URL" once
        // `domain::SubtitleHit` carries the OpenSubtitles details
        // URL (`attributes.url` in the JSON payload, zero extra
        // network cost). See plan: `/home/tlm/.pi/plans/context-menus.md`.
        KinemaMenuItem {
            iconName: "copy"
            label: i18nc("@action:inmenu subtitle row", "Copy Release")
            enabled: card.release.length > 0
            onTriggered: shell.copyToClipboard(card.release,
                i18nc("@info:status",
                    "Release name copied to clipboard"))
        }
        KinemaMenuItem {
            iconName: "copy"
            label: i18nc("@action:inmenu subtitle row", "Copy Filename")
            enabled: card.fileName.length > 0
            onTriggered: shell.copyToClipboard(card.fileName,
                i18nc("@info:status",
                    "File name copied to clipboard"))
        }
    }
}
