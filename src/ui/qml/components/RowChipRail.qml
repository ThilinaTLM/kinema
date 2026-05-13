// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Horizontal rail of `MetaChip`s with uniform rhythm. Used by
// `StreamListCard`, `DownloadListCard`, `SubtitleListCard` to render
// their chip strips consistently.
//
// `chips` is a JS array of plain objects:
//
//     { text: "1080p", tone: "neutral",
//       iconSource: "image://...", iconColor: "..." }
//
// Empty-`text` entries are skipped so cards can pass conditional
// chips inline without `.filter()` boilerplate at the call site.
//
// An opt-in default-property slot lets a card append an ad-hoc
// non-chip element (e.g. a download-rate Label in DownloadListCard)
// to the rail. The slot sits flush after the chip flow.
//
// The rail sizes to its content and packs flush left. It does NOT
// install an internal `Layout.fillWidth` spacer: when a rail like
// this is placed inside another `RowLayout` next to siblings, an
// internal fill child propagates as a fill hint on the rail itself,
// stretches it across the parent's width, and shoves every sibling
// after the rail to the right edge. Consumers that want the rail to
// occupy the whole row set `Layout.fillWidth: true` on the rail
// instance; the trailing slack then simply sits at the end of the
// rail's allocation (RowLayout's default start-aligned packing),
// which is exactly what we want.
RowLayout {
    id: rail

    /// JS array of `{ text, tone, iconSource?, iconColor? }`
    /// descriptors. Empty-`text` entries render nothing.
    property var chips: []

    /// Trailing in-rail slot for ad-hoc content. Children declared
    /// inside the consumer become children of `trailingHost`; layout
    /// is a RowLayout so multiple items sit side-by-side.
    default property alias trailing: trailingHost.data

    spacing: Kirigami.Units.largeSpacing

    Repeater {
        model: rail.chips

        delegate: MetaChip {
            required property var modelData
            visible: modelData
                && modelData.text !== undefined
                && ("" + modelData.text).length > 0
            text: visible ? modelData.text : ""
            tone: (modelData && modelData.tone) || "neutral"
            iconSource: modelData ? modelData.iconSource : undefined
            iconColor: (modelData && modelData.iconColor)
                || labelColor
        }
    }

    // Trailing slot — flush after the last chip so a card's "rate
    // label" lines up next to the chip rhythm without a gap.
    RowLayout {
        id: trailingHost
        Layout.alignment: Qt.AlignVCenter
        visible: trailingHost.children.length > 0
        spacing: Theme.inlineSpacing
    }
}
