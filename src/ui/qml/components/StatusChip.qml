// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// "Watched" / "Upcoming" pill used on Library cards. Mirrors the
// translucent pill styling of `RatingChip` and `EpisodeChip` so the
// three overlay chips read as a coherent family.
//
// The chip self-hides when both flags are false. `watched` wins
// over `upcoming` when both are set (a fully-watched upcoming entry
// shouldn't really happen in practice, but the precedence keeps the
// UI deterministic if the model glitches).
Rectangle {
    id: chip

    property bool watched: false
    property bool upcoming: false

    readonly property bool _isWatched: chip.watched
    readonly property bool _isUpcoming: chip.upcoming && !chip.watched

    visible: chip._isWatched || chip._isUpcoming

    radius: Kirigami.Units.cornerRadius
    color: Qt.alpha(Theme.background, 0.78)
    border.color: Qt.alpha(Theme.foreground, 0.18)
    border.width: 1

    implicitWidth: row.implicitWidth + Kirigami.Units.smallSpacing * 2
    implicitHeight: row.implicitHeight + Kirigami.Units.smallSpacing

    RowLayout {
        id: row
        anchors.centerIn: parent
        spacing: 2

        Kirigami.Icon {
            Layout.preferredWidth: Kirigami.Units.iconSizes.small
            Layout.preferredHeight: Kirigami.Units.iconSizes.small
            source: AppIcons.url(chip._isWatched
                ? "circle-check"
                : "clock-arrow-down")
            color: chip._isWatched ? Theme.positive : Theme.accent
        }

        QQC2.Label {
            text: chip._isWatched
                ? i18nc("@label library card badge", "Watched")
                : i18nc("@label library card badge", "Upcoming")
            font.pointSize: Theme.captionFont.pointSize
            font.bold: true
            color: Theme.foreground
        }
    }
}
