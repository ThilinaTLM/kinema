// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Title + meta row + genres + description block used inside
// `BackdropHero`. The host arranges this column to the right of the
// poster and lets it consume the remaining width.
//
// Inputs are bound through plain properties so the same component
// could be lifted into the SeriesDetailPage (commit B) without
// touching its shape.
ColumnLayout {
    id: meta

    spacing: Kirigami.Units.smallSpacing

    property string title
    property int year: 0
    property var genres: []
    property int runtimeMinutes: 0
    property real rating: -1
    property string description
    property bool isUpcoming: false
    property string releaseDateText

    // ---- title --------------------------------------------------
    Kirigami.Heading {
        Layout.fillWidth: true
        level: 1
        text: meta.year > 0
            ? i18nc("@title detail", "%1 (%2)", meta.title, meta.year)
            : meta.title
        wrapMode: Text.Wrap
        maximumLineCount: 2
        elide: Text.ElideRight
        color: Theme.foreground
    }

    // ---- single-line meta strip --------------------------------
    // genres · runtime · rating · upcoming
    RowLayout {
        Layout.fillWidth: true
        spacing: Kirigami.Units.smallSpacing
        visible: genresLabel.text.length > 0 || runtimeLabel.text.length > 0
            || ratingLabel.text.length > 0 || upcomingChip.visible

        QQC2.Label {
            id: genresLabel
            text: meta.genres.length > 0
                ? meta.genres.slice(0, 3).join(" \u00b7 ")
                : ""
            elide: Text.ElideRight
            font.pointSize: Theme.defaultFont.pointSize
            color: Theme.foreground
            visible: text.length > 0
            Layout.maximumWidth: meta.width / 2
        }
        QQC2.Label {
            text: "\u00b7"
            color: Theme.disabled
            visible: genresLabel.visible
                && (runtimeLabel.visible || ratingLabel.visible
                    || upcomingChip.visible)
        }
        QQC2.Label {
            id: runtimeLabel
            text: meta.runtimeMinutes > 0
                ? i18nc("@info runtime hours+minutes",
                    "%1 h %2 m",
                    Math.floor(meta.runtimeMinutes / 60),
                    String(meta.runtimeMinutes % 60).padStart(2, "0"))
                : ""
            font.pointSize: Theme.defaultFont.pointSize
            color: Theme.foreground
            visible: text.length > 0
        }
        QQC2.Label {
            text: "\u00b7"
            color: Theme.disabled
            visible: runtimeLabel.visible
                && (ratingLabel.visible || upcomingChip.visible)
        }
        QQC2.Label {
            id: ratingLabel
            text: meta.rating > 0
                ? i18nc("@info imdb rating",
                    "\u2605 %1", meta.rating.toFixed(1))
                : ""
            font.pointSize: Theme.defaultFont.pointSize
            font.weight: Font.DemiBold
            color: Theme.foreground
            visible: text.length > 0
        }
        UpcomingBadge {
            id: upcomingChip
            visible: meta.isUpcoming
            releaseDateText: meta.releaseDateText
        }
        Item { Layout.fillWidth: true }
    }

    // ---- description ------------------------------------------
    QQC2.Label {
        Layout.fillWidth: true
        Layout.topMargin: Kirigami.Units.smallSpacing
        text: meta.description
        wrapMode: Text.Wrap
        maximumLineCount: 6
        elide: Text.ElideRight
        font.pointSize: Theme.defaultFont.pointSize
        color: Theme.foreground
        visible: text.length > 0
    }
}
