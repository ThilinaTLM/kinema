// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// "Coming soon" pill rendered next to the rating row in the detail
// hero (or inline in an episode list). Phase 06's MetaHeaderWidget
// migration retires the C++ `UpcomingBadge` painter; this is the
// QML replacement consumed by `MetaSection.qml` and (commit B's)
// `EpisodeRow.qml`.
//
// `releaseDateText` is the already-formatted date string supplied
// by the view-model (which routes through `core::formatReleaseDate`
// on the C++ side so the day/month/year ordering matches every
// other surface).
QQC2.Control {
    id: badge

    property string releaseDateText

    leftPadding: Kirigami.Units.smallSpacing * 2
    rightPadding: Kirigami.Units.smallSpacing * 2
    topPadding: Math.round(Kirigami.Units.smallSpacing / 2)
    bottomPadding: Math.round(Kirigami.Units.smallSpacing / 2)

    background: Rectangle {
        radius: badge.height / 2
        color: Qt.alpha(Theme.accent, 0.18)
        border.color: Theme.accent
        border.width: 1
    }

    contentItem: QQC2.Label {
        text: badge.releaseDateText.length > 0
            ? i18nc("@info upcoming with release date",
                "Upcoming \u00b7 %1", badge.releaseDateText)
            : i18nc("@info upcoming without date", "Upcoming")
        color: Theme.accent
        font.pointSize: Theme.captionFont.pointSize
        font.weight: Font.DemiBold
        elide: Text.ElideRight
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }
}
