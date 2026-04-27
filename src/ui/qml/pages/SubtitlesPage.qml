// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// Standalone subtitles search/download page. Detail pages normally embed the
// same SubtitlesPanel inline; this page remains for player-initiated downloads
// and any context that does not expose an inline subtitles panel.
Kirigami.Page {
    id: page

    objectName: "subtitles"
    padding: 0
    title: i18nc("@title:window subtitles search page",
        "Subtitles for %1", subtitlesVm.contextTitle)

    actions: [
        Kirigami.Action {
            icon.name: "configure"
            text: i18nc("@action subtitles page header, open settings",
                "Open settings…")
            onTriggered: subtitlesVm.openSettings()
        }
    ]

    SubtitlesPanel {
        anchors.fill: parent
        vm: subtitlesVm
        showContextTitle: false
        showCancelButton: true
        compact: false
    }
}
