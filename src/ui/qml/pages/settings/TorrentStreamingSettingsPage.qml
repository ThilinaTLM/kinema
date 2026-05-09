// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

FormCard.FormCardPage {
    id: page
    title: i18nc("@title:tab settings page", "Torrent streaming")

    readonly property var vm: settingsVm.torrentStreaming

    FormCard.FormHeader {
        title: i18nc("@title:group torrent streaming settings", "Cache")
    }
    FormCard.FormCard {
        FormCard.FormSpinBoxDelegate {
            label: i18nc("@label:spinbox", "Cache budget")
            from: 1
            to: 1024
            value: page.vm.cacheBudgetGb
            textFromValue: value => i18ncp("@label cache size", "%1 GB", "%1 GB", value)
            valueFromText: text => parseInt(text)
            onValueChanged: page.vm.cacheBudgetGb = value
        }
        FormCard.FormTextDelegate {
            text: i18nc("@info torrent cache description",
                "Torrent data is cached automatically and pruned least-recently-used when the budget is exceeded.")
        }
    }

    FormCard.FormHeader {
        title: i18nc("@title:group torrent streaming settings", "Streaming behavior")
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

    FormCard.FormHeader {
        title: i18nc("@title:group torrent streaming settings", "Bandwidth")
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
                "Torrent streaming uses public BitTorrent peers. Kinema stops sessions after playback/idle timeout and does not seed in the background.")
        }
    }
}
