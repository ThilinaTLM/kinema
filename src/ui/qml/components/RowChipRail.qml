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
// to the rail. The slot sits after the chip flow and before the
// trailing fill spacer, so a card can append "live transient"
// information without the chip rhythm.
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

    // Trailing slot — sits in the rail before the spacer so a card's
    // "rate label" lines up next to the last chip.
    RowLayout {
        id: trailingHost
        Layout.alignment: Qt.AlignVCenter
        visible: trailingHost.children.length > 0
        spacing: Theme.inlineSpacing
    }

    // Flush remaining slack to the right so the chips don't expand.
    Item { Layout.fillWidth: true }
}
