// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

import dev.tlmtech.kinema.app

// State-switched list container. Wraps a model-driven `ListView` and
// the six placeholder bodies (Idle / Loading / Ready / Empty / Error
// / Custom) the app uses on Streams, Subtitles, and Downloads.
//
// Pages bind their native state to `state` via a one-line mapping,
// e.g.
//
//     state: ({
//         "loading": ListSurface.Loading,
//         "ready":   ListSurface.Ready,
//     }[vm.state] ?? ListSurface.Idle)
//
// Placeholder bodies are described by JS-object configs (`icon`,
// `iconColor`, `title`, `explanation`, `actionText`, `actionIcon`,
// `onActionTriggered`) on `idleConfig` / `loadingConfig` /
// `emptyConfig` / `errorConfig`. `customComponent` is the escape
// hatch for placeholders that don't fit the title / explanation /
// action shape (e.g. StreamsPage's "Not released yet").
//
// Optional `footer` slot hosts a sticky toolbar (SubtitlesPanel's
// Open file… / primary action bar).
Item {
    id: surface

    enum State {
        Idle = 0,
        Loading,
        Ready,
        Empty,
        Error,
        Custom
    }

    // ---- Inputs --------------------------------------------------
    property int state: ListSurface.Idle

    property var model
    property Component delegate

    property int listSpacing: Theme.listRowSpacing
    property int listLeftMargin: Theme.pageMargin
    property int listRightMargin: Theme.pageMargin
    property int listTopMargin: Theme.inlineSpacing
    property int listBottomMargin: Theme.groupSpacing
    property int currentIndex: -1
    property int cacheBuffer: Kirigami.Units.gridUnit * 20

    property string sectionProperty: ""
    property int sectionCriteria: ViewSection.FullString
    property Component sectionDelegate

    // Placeholder configs. `null` => Idle is blank, Loading is a
    // centred BusyIndicator, Empty / Error are blank.
    property var idleConfig: null
    property var loadingConfig: null
    property var emptyConfig: null
    property var errorConfig: null

    property Component customComponent: null
    property Component footer: null

    /// Exposes the inner ListView for callers that need to scroll-
    /// to-index or otherwise reach into it. Most pages don't.
    readonly property alias listView: list

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        StackLayout {
            id: stack
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: surface.state

            // 0 — Idle
            Item {
                PlaceholderHost {
                    anchors.centerIn: parent
                    hostWidth: parent.width
                    config: surface.idleConfig
                }
            }

            // 1 — Loading
            Item {
                PlaceholderHost {
                    anchors.centerIn: parent
                    hostWidth: parent.width
                    config: surface.loadingConfig
                    visible: surface.loadingConfig !== null
                }
                QQC2.BusyIndicator {
                    anchors.centerIn: parent
                    visible: surface.loadingConfig === null
                    running: visible && stack.currentIndex === 1
                }
            }

            // 2 — Ready: virtualised list
            ListView {
                id: list
                model: surface.model
                clip: true
                spacing: surface.listSpacing
                leftMargin: surface.listLeftMargin
                rightMargin: surface.listRightMargin
                topMargin: surface.listTopMargin
                bottomMargin: surface.listBottomMargin
                cacheBuffer: surface.cacheBuffer
                currentIndex: surface.currentIndex
                boundsBehavior: Flickable.StopAtBounds
                delegate: surface.delegate

                section.property: surface.sectionProperty
                section.criteria: surface.sectionCriteria
                section.delegate: surface.sectionDelegate
                    ? surface.sectionDelegate
                    : null
            }

            // 3 — Empty
            Item {
                PlaceholderHost {
                    anchors.centerIn: parent
                    hostWidth: parent.width
                    config: surface.emptyConfig
                }
            }

            // 4 — Error
            Item {
                PlaceholderHost {
                    anchors.centerIn: parent
                    hostWidth: parent.width
                    config: surface.errorConfig
                }
            }

            // 5 — Custom
            Item {
                Loader {
                    anchors.centerIn: parent
                    active: surface.customComponent !== null
                    sourceComponent: surface.customComponent
                }
            }
        }

        Loader {
            Layout.fillWidth: true
            active: surface.footer !== null
            visible: active
            sourceComponent: surface.footer
        }
    }

    // Inline `PlaceholderHost` declared via an in-file Component so
    // each state branch can pass its own `config` object cleanly.
    component PlaceholderHost: Kirigami.PlaceholderMessage {
        property var config: null
        property real hostWidth: 0

        visible: config !== null
            && config.title !== undefined
            && ("" + config.title).length > 0
        width: Math.min(hostWidth - Theme.pageWideMargin * 2,
            Theme.placeholderMaxWidth)
        icon.source: visible && config.icon
            ? AppIcons.url(config.icon)
            : ""
        icon.color: visible && config.iconColor
            ? config.iconColor
            : AppIcons.foreground
        text: visible ? ("" + config.title) : ""
        explanation: visible && config.explanation
            ? ("" + config.explanation)
            : ""

        helpfulAction: (visible
                && config.actionText
                && ("" + config.actionText).length > 0)
            ? helpAction
            : null

        Kirigami.Action {
            id: helpAction
            text: (config && config.actionText)
                ? ("" + config.actionText)
                : ""
            icon.source: (config && config.actionIcon)
                ? AppIcons.url(config.actionIcon)
                : ""
            icon.color: AppIcons.foreground
            onTriggered: {
                if (config && config.onActionTriggered) {
                    config.onActionTriggered();
                }
            }
        }
    }
}
