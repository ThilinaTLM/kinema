// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.kirigamiaddons.formcard as FormCard

import dev.tlmtech.kinema.app

FormCard.FormCardPage {
    title: i18nc("@title:tab settings page", "Player")

    readonly property var vm: settingsVm.player

    FormCard.FormHeader {
        title: i18nc("@title:group", "Preferred player")
    }
    FormCard.FormCard {
        FormCard.FormRadioDelegate {
            text: vm.embeddedAvailable
                ? i18nc("@option:radio player kind", "Embedded (built-in mpv)")
                : i18nc("@option:radio player kind, build lacks libmpv",
                    "Embedded (not built with libmpv)")
            description: i18nc("@info form delegate hint",
                "Plays inside Kinema using Kinema's own mpv configuration.")
            checked: vm.preferredPlayer === 0
            enabled: vm.embeddedAvailable
            onToggled: if (checked) vm.preferredPlayer = 0
        }
        FormCard.FormRadioDelegate {
            text: vm.mpvAvailable
                ? i18nc("@option:radio player kind", "mpv")
                : i18nc("@option:radio player kind, missing",
                    "mpv (not found on PATH)")
            checked: vm.preferredPlayer === 1
            onToggled: if (checked) vm.preferredPlayer = 1
        }
        FormCard.FormRadioDelegate {
            text: vm.vlcAvailable
                ? i18nc("@option:radio player kind", "VLC")
                : i18nc("@option:radio player kind, missing",
                    "VLC (not found on PATH)")
            checked: vm.preferredPlayer === 2
            onToggled: if (checked) vm.preferredPlayer = 2
        }
        FormCard.FormRadioDelegate {
            text: i18nc("@option:radio player kind", "Custom command")
            checked: vm.preferredPlayer === 3
            onToggled: if (checked) vm.preferredPlayer = 3
        }
        FormCard.FormTextFieldDelegate {
            label: i18nc("@label:textbox", "Custom command")
            placeholderText: i18nc("@info:placeholder",
                "e.g. mpv --fs --really-quiet {url}")
            text: vm.customCommand
            onTextChanged: vm.customCommand = text
            enabled: vm.preferredPlayer === 3
        }
    }

    FormCard.FormHeader {
        title: i18nc("@title:group player embedded options", "Embedded player")
        visible: vm.embeddedAvailable
    }
    FormCard.FormCard {
        visible: vm.embeddedAvailable
        FormCard.FormSwitchDelegate {
            text: i18nc("@option:check", "Use hardware decoding")
            description: i18nc("@info form delegate hint",
                "Disable only if you see driver issues. Software decoding "
                + "works everywhere but uses more CPU.")
            checked: vm.hardwareDecoding
            onToggled: vm.hardwareDecoding = checked
        }
        FormCard.FormDelegateSeparator {}
        FormCard.FormTextFieldDelegate {
            label: i18nc("@label:textbox", "Preferred audio language")
            placeholderText: i18nc("@info:placeholder", "e.g. en,ja")
            text: vm.preferredAudioLang
            onTextChanged: vm.preferredAudioLang = text
        }
        FormCard.FormTextFieldDelegate {
            label: i18nc("@label:textbox", "Preferred subtitle language")
            placeholderText: i18nc("@info:placeholder", "e.g. en")
            text: vm.preferredSubtitleLang
            onTextChanged: vm.preferredSubtitleLang = text
        }
    }

    FormCard.FormHeader {
        title: i18nc("@title:group player series options", "Series")
        visible: vm.embeddedAvailable
    }
    FormCard.FormCard {
        visible: vm.embeddedAvailable
        FormCard.FormSwitchDelegate {
            text: i18nc("@option:check", "Autoplay next episode")
            checked: vm.autoplayNextEpisode
            onToggled: vm.autoplayNextEpisode = checked
        }
        FormCard.FormSwitchDelegate {
            text: i18nc("@option:check", "Offer skip intro / credits")
            checked: vm.skipIntroChapters
            onToggled: vm.skipIntroChapters = checked
        }
        FormCard.FormSpinBoxDelegate {
            label: i18nc("@label:spinbox", "Resume prompt threshold (s)")
            value: vm.resumePromptThresholdSec
            from: 0
            to: 24 * 60 * 60
            onValueChanged: vm.resumePromptThresholdSec = value
        }
        FormCard.FormSpinBoxDelegate {
            label: i18nc("@label:spinbox", "Next-episode countdown (s)")
            value: vm.autoNextCountdownSec
            from: 0
            to: 600
            onValueChanged: vm.autoNextCountdownSec = value
        }
    }
}
