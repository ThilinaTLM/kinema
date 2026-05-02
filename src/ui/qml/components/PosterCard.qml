// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// One TMDB / Cinemeta tile: 2:3 poster, single-line title, caption
// subtitle, corner rating chip. Shares chrome with every other
// poster/thumbnail card in the app via `KinemaArtworkFrame` —
// hover lift, animated focus ring, hover tint, and fallback icon
// all live there. This card just composes the frame with the
// rating chip overlay and the meta block, and wires activation.
//
// Public surface is intentionally identical to the previous
// version so `PosterGrid`, `ContentRail`, and `SimilarCarousel`
// can keep using the delegate without changes.
Item {
    id: card

    // ---- Inputs --------------------------------------------------
    property string posterUrl
    property string title
    property string subtitle
    property real rating: -1

    signal clicked()

    // Single source of truth for hover state so the frame's
    // hover/focus inputs and any future overlay tweaks stay in
    // sync.
    readonly property bool _hovered: hoverHandler.hovered

    Kirigami.Theme.colorSet: Kirigami.Theme.View
    Kirigami.Theme.inherit: false

    implicitWidth: Theme.posterMin
    implicitHeight: poster.height + meta.implicitHeight
        + Kirigami.Units.smallSpacing * 2

    // Keyboard focus support: the parent grid forwards arrow keys
    // through `GridView` and we only need to react to activation.
    activeFocusOnTab: true
    Keys.onPressed: function (event) {
        if (event.key === Qt.Key_Return
            || event.key === Qt.Key_Enter
            || event.key === Qt.Key_Space) {
            card.clicked();
            event.accepted = true;
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: Kirigami.Units.smallSpacing

        KinemaArtworkFrame {
            id: poster
            Layout.fillWidth: true
            Layout.preferredHeight: Math.round(width * aspect)

            url: card.posterUrl
            aspect: 1.5
            fallbackIcon: "film"
            hovered: card._hovered
            focusRing: card._hovered || card.activeFocus

            // Rating overlay (top-right). Sits inside the inset so
            // it never clips the rounded corner.
            RatingChip {
                rating: card.rating
                anchors {
                    top: parent.top
                    right: parent.right
                    topMargin: Kirigami.Units.smallSpacing
                    rightMargin: Kirigami.Units.smallSpacing
                }
            }
        }

        ColumnLayout {
            id: meta
            spacing: 0
            Layout.fillWidth: true

            Kirigami.Heading {
                Layout.fillWidth: true
                level: 5
                text: card.title
                elide: Text.ElideRight
                maximumLineCount: 1
                color: Kirigami.Theme.textColor
            }

            QQC2.Label {
                Layout.fillWidth: true
                visible: card.subtitle.length > 0
                text: card.subtitle
                elide: Text.ElideRight
                font: Kirigami.Theme.smallFont
                color: Kirigami.Theme.disabledTextColor
            }
        }
    }

    // Click + hover handlers. TapHandler covers click + keyboard
    // synthesised activation via `Keys.onPressed` above.
    TapHandler {
        acceptedButtons: Qt.LeftButton
        onTapped: card.clicked()
    }
    HoverHandler {
        id: hoverHandler
        cursorShape: Qt.PointingHandCursor
    }
}
