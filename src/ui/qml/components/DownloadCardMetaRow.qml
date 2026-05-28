// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Line 2 of a `DownloadListCard` body: the dot-separated caption
// row — State (toned) · Quality · Backend (icon + label) · status ·
// rate. Pure presentation: `DownloadListCard` computes every token
// (including the toned state colour) and hands it down, so the row's
// single caption-sized line height stays identical for every download
// state regardless of which optional tokens are present.
RowLayout {
    id: root

    required property string stateText
    required property color stateColor
    required property string qualityText
    required property string backendIcon
    required property string backendLabel
    required property string statusText
    required property bool statusIsError
    /// Live download rate. The card passes "" when the transfer is
    /// complete, so the rate token simply collapses.
    required property string rateText

    readonly property bool hasState: stateText.length > 0
    readonly property bool hasQuality: qualityText.length > 0
    readonly property bool hasBackend: backendLabel.length > 0
    readonly property bool hasStatus: statusText.length > 0
    readonly property bool hasRate: rateText.length > 0

    spacing: Theme.inlineSpacing

    // State — tone moves from chip border to label colour.
    QQC2.Label {
        Layout.alignment: Qt.AlignVCenter
        visible: root.hasState
        text: root.stateText
        font.pointSize: Theme.captionFont.pointSize
        color: root.stateColor
        verticalAlignment: Text.AlignVCenter
    }

    // (state) → (quality)
    QQC2.Label {
        Layout.alignment: Qt.AlignVCenter
        visible: root.hasState && root.hasQuality
        text: "\u00b7"
        font.pointSize: Theme.captionFont.pointSize
        color: Theme.disabled
        verticalAlignment: Text.AlignVCenter
    }

    // Quality (e.g. `1080p WEB-DL` / `720p`).
    QQC2.Label {
        Layout.alignment: Qt.AlignVCenter
        visible: root.hasQuality
        text: root.qualityText
        font.pointSize: Theme.captionFont.pointSize
        color: Theme.disabled
        verticalAlignment: Text.AlignVCenter
    }

    // (state|quality) → (backend)
    QQC2.Label {
        Layout.alignment: Qt.AlignVCenter
        visible: (root.hasState || root.hasQuality) && root.hasBackend
        text: "\u00b7"
        font.pointSize: Theme.captionFont.pointSize
        color: Theme.disabled
        verticalAlignment: Text.AlignVCenter
    }

    // Backend icon. Sized down from `iconSizes.small` so it sits on
    // the caption baseline next to the label rather than dominating
    // the line.
    Kirigami.Icon {
        Layout.alignment: Qt.AlignVCenter
        visible: root.hasBackend && root.backendIcon.length > 0
        Layout.preferredWidth:
            Math.round(Kirigami.Units.iconSizes.small * 0.8)
        Layout.preferredHeight: width
        source: root.backendIcon
        color: Theme.disabled
    }

    // Backend label.
    QQC2.Label {
        Layout.alignment: Qt.AlignVCenter
        visible: root.hasBackend
        text: root.backendLabel
        font.pointSize: Theme.captionFont.pointSize
        color: Theme.disabled
        verticalAlignment: Text.AlignVCenter
    }

    // (state|quality|backend) → (status)
    QQC2.Label {
        Layout.alignment: Qt.AlignVCenter
        visible: (root.hasState || root.hasQuality || root.hasBackend)
            && root.hasStatus
        text: "\u00b7"
        font.pointSize: Theme.captionFont.pointSize
        color: Theme.disabled
        verticalAlignment: Text.AlignVCenter
    }

    // Status caption — recolours `Theme.negative` on failed rows (the
    // `⚠` prefix is baked into the status text by the card).
    QQC2.Label {
        Layout.alignment: Qt.AlignVCenter
        Layout.fillWidth: false
        visible: root.hasStatus
        text: root.statusText
        elide: Text.ElideRight
        font.pointSize: Theme.captionFont.pointSize
        color: root.statusIsError ? Theme.negative : Theme.disabled
        verticalAlignment: Text.AlignVCenter
    }

    // (status) → (rate)
    QQC2.Label {
        Layout.alignment: Qt.AlignVCenter
        visible: (root.hasState || root.hasQuality || root.hasBackend
            || root.hasStatus) && root.hasRate
        text: "\u00b7"
        font.pointSize: Theme.captionFont.pointSize
        color: Theme.disabled
        verticalAlignment: Text.AlignVCenter
    }

    // Live download rate — the only token in foreground / DemiBold so
    // it scans as the row's live metric.
    QQC2.Label {
        Layout.alignment: Qt.AlignVCenter
        visible: root.hasRate
        text: root.rateText
        font.pointSize: Theme.captionFont.pointSize
        color: Theme.foreground
        font.weight: Font.DemiBold
        verticalAlignment: Text.AlignVCenter
    }

    // Trailing fill keeps the row packed flush left when the body
    // stretches it to the card width.
    Item { Layout.fillWidth: true }
}
