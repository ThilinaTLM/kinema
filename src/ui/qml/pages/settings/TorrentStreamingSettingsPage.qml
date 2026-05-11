// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

// "Downloads" settings page \u2014 cache budget, live cache usage,
// streaming buffers, bandwidth caps. Despite the historical class
// name (`TorrentStreamingSettingsViewModel`, file
// `TorrentStreamingSettings.qml`), this page covers both backends:
// the libtorrent-backed cache and the Real-Debrid HTTP cache share
// the same `MediaCache` and `DownloadSettings`. The user-facing tab
// label in `ApplicationShell.qml` is "Downloads".
FormCard.FormCardPage {
    id: page
    title: i18nc("@title:tab settings page", "Downloads")

    readonly property var vm: settingsVm.torrentStreaming

    // ---- Cache usage (live) -----------------------------------
    FormCard.FormHeader {
        title: i18nc("@title:group downloads settings", "Cache")
    }
    FormCard.FormCard {
        FormCard.AbstractFormDelegate {
            background: null
            contentItem: ColumnLayout {
                spacing: Kirigami.Units.smallSpacing

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.largeSpacing

                    QQC2.Label {
                        text: i18nc("@label cache usage row",
                            "Cache used")
                        Layout.alignment: Qt.AlignVCenter
                    }

                    QQC2.ProgressBar {
                        Layout.fillWidth: true
                        Layout.alignment: Qt.AlignVCenter
                        from: 0
                        to: 1
                        value: page.vm.cacheUsageFraction
                    }

                    QQC2.Label {
                        Layout.alignment: Qt.AlignVCenter
                        text: i18nc("@label cache used / budget",
                            "%1 / %2",
                            page.vm.cacheSizeText,
                            page.vm.cacheBudgetText)
                        font.weight: Font.DemiBold
                    }
                }

                QQC2.Label {
                    Layout.fillWidth: true
                    text: i18nc("@info ephemeral vs pinned cache breakdown",
                        "Ephemeral cache: %1 \u00b7 pinned items don't "
                        + "count toward the budget.",
                        page.vm.ephemeralSizeText)
                    color: Kirigami.Theme.disabledTextColor
                    font.pointSize: Kirigami.Theme.smallFont.pointSize
                    wrapMode: Text.WordWrap
                }
            }
        }
        FormCard.FormDelegateSeparator {}
        FormCard.FormButtonDelegate {
            text: i18nc("@action:button run cache eviction now",
                "Run eviction now")
            description: i18nc("@info cache eviction description",
                "Drop ephemeral cache entries until the budget is "
                + "satisfied. Pinned items are never evicted.")
            icon.name: "edit-clear-history"
            onClicked: page.vm.runEvictionNow()
        }
    }

    // ---- Cache budget ------------------------------------------
    FormCard.FormHeader {
        title: i18nc("@title:group downloads settings",
            "Cache budget")
    }
    FormCard.FormCard {
        FormCard.FormSpinBoxDelegate {
            label: i18nc("@label:spinbox", "Cache budget")
            from: 1
            to: 1024
            value: page.vm.cacheBudgetGb
            textFromValue: value => i18ncp("@label cache size",
                "%1 GB", "%1 GB", value)
            valueFromText: text => parseInt(text)
            onValueChanged: page.vm.cacheBudgetGb = value
        }
        FormCard.FormTextDelegate {
            text: i18nc("@info cache description",
                "Cached download data is pruned least-recently-used "
                + "when the budget is exceeded.")
        }
    }

    // ---- Streaming behavior ------------------------------------
    FormCard.FormHeader {
        title: i18nc("@title:group downloads settings",
            "Streaming behavior")
    }
    FormCard.FormCard {
        FormCard.FormSpinBoxDelegate {
            label: i18nc("@label:spinbox", "Startup buffer")
            from: 1
            to: 4096
            value: page.vm.startupBufferMiB
            textFromValue: value => i18ncp("@label buffer size", "%1 MiB", "%1 MiB", value)
            valueFromText: text => parseInt(text)
            onValueChanged: page.vm.startupBufferMiB = value
        }
        FormCard.FormSpinBoxDelegate {
            label: i18nc("@label:spinbox", "Readahead")
            from: 1
            to: 8192
            value: page.vm.readaheadMiB
            textFromValue: value => i18ncp("@label buffer size", "%1 MiB", "%1 MiB", value)
            valueFromText: text => parseInt(text)
            onValueChanged: page.vm.readaheadMiB = value
        }
        FormCard.FormSpinBoxDelegate {
            label: i18nc("@label:spinbox", "Tail prebuffer")
            from: 0
            to: 1024
            value: page.vm.tailBufferMiB
            textFromValue: value => i18ncp("@label buffer size", "%1 MiB", "%1 MiB", value)
            valueFromText: text => parseInt(text)
            onValueChanged: page.vm.tailBufferMiB = value
        }
        FormCard.FormSpinBoxDelegate {
            label: i18nc("@label:spinbox", "Stop idle sessions after")
            from: 1
            to: 240
            value: page.vm.idleStopMinutes
            textFromValue: value => i18ncp("@label minutes", "%1 minute", "%1 minutes", value)
            valueFromText: text => parseInt(text)
            onValueChanged: page.vm.idleStopMinutes = value
        }
    }

    // ---- Bandwidth ---------------------------------------------
    FormCard.FormHeader {
        title: i18nc("@title:group downloads settings", "Bandwidth")
    }
    FormCard.FormCard {
        FormCard.FormSpinBoxDelegate {
            label: i18nc("@label:spinbox", "Download limit")
            from: 0
            to: 1048576
            value: page.vm.maxDownloadRateKiB
            textFromValue: value => value <= 0
                ? i18nc("@label unlimited bandwidth", "Unlimited")
                : i18ncp("@label bandwidth", "%1 KiB/s", "%1 KiB/s", value)
            valueFromText: text => parseInt(text) || 0
            onValueChanged: page.vm.maxDownloadRateKiB = value
        }
        FormCard.FormSpinBoxDelegate {
            label: i18nc("@label:spinbox", "Upload limit while streaming")
            from: 0
            to: 1048576
            value: page.vm.maxUploadRateKiB
            textFromValue: value => value <= 0
                ? i18nc("@label unlimited bandwidth", "Unlimited")
                : i18ncp("@label bandwidth", "%1 KiB/s", "%1 KiB/s", value)
            valueFromText: text => parseInt(text) || 0
            onValueChanged: page.vm.maxUploadRateKiB = value
        }
        FormCard.FormTextDelegate {
            text: i18nc("@info torrent privacy note",
                "Torrent streaming uses public BitTorrent peers. "
                + "Kinema stops sessions after playback/idle timeout "
                + "and does not seed in the background.")
        }
    }
}
