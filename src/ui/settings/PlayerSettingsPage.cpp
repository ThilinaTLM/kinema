// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/settings/PlayerSettingsPage.h"

#include "config/Config.h"
#include "core/Player.h"

#include <KLocalizedString>

#include <QButtonGroup>
#include <QFormLayout>
#include <QLabel>
#include <QLineEdit>
#include <QRadioButton>
#include <QVBoxLayout>

namespace kinema::ui::settings {

namespace {

QString labelWithAvailability(const QString& base, core::player::Kind kind)
{
    if (core::player::isAvailable(kind)) {
        return base;
    }
    return i18nc("@option:radio label with availability note",
        "%1 (not found on PATH)", base);
}

} // namespace

PlayerSettingsPage::PlayerSettingsPage(QWidget* parent)
    : QWidget(parent)
{
    // Embedded (in-app mpv) is the "nicest" option when available, so
    // it leads the list. When Kinema was built without libmpv the
    // radio is disabled with an explanatory suffix; we don't remove
    // it so users understand the option exists.
    const bool embeddedAvailable = core::player::isAvailable(
        core::player::Kind::Embedded);
    QString embeddedLabel = core::player::displayName(
        core::player::Kind::Embedded);
    if (!embeddedAvailable) {
        embeddedLabel = i18nc("@option:radio label when build lacks libmpv",
            "%1 (not built with libmpv)", embeddedLabel);
    }
    m_embeddedRadio = new QRadioButton(embeddedLabel, this);
    m_embeddedRadio->setEnabled(embeddedAvailable);
    m_embeddedRadio->setToolTip(i18nc("@info:tooltip",
        "Plays inside Kinema. Uses your local mpv config and keybindings."));

    m_mpvRadio = new QRadioButton(
        labelWithAvailability(core::player::displayName(core::player::Kind::Mpv),
            core::player::Kind::Mpv),
        this);
    m_vlcRadio = new QRadioButton(
        labelWithAvailability(core::player::displayName(core::player::Kind::Vlc),
            core::player::Kind::Vlc),
        this);
    m_customRadio = new QRadioButton(
        i18nc("@option:radio", "Custom command"), this);

    auto* group = new QButtonGroup(this);
    group->addButton(m_embeddedRadio);
    group->addButton(m_mpvRadio);
    group->addButton(m_vlcRadio);
    group->addButton(m_customRadio);

    m_customCmdEdit = new QLineEdit(this);
    m_customCmdEdit->setPlaceholderText(
        i18nc("@info:placeholder",
            "e.g. mpv --fs --really-quiet {url}"));

    auto* customHint = new QLabel(
        i18nc("@info",
            "Use {url} where the stream URL should appear; otherwise the "
            "URL is appended as the last argument."),
        this);
    customHint->setWordWrap(true);
    customHint->setEnabled(false);

    auto* form = new QFormLayout;
    form->addRow(m_embeddedRadio);
    form->addRow(m_mpvRadio);
    form->addRow(m_vlcRadio);
    form->addRow(m_customRadio);
    form->addRow(i18nc("@label:textbox", "Custom command:"), m_customCmdEdit);
    form->addRow(QString {}, customHint);

    auto* layout = new QVBoxLayout(this);
    layout->addLayout(form);
    layout->addStretch(1);

    connect(m_customRadio, &QRadioButton::toggled,
        this, [this] { updateCustomEnabled(); });

    load();
}

void PlayerSettingsPage::load()
{
    const auto& cfg = config::Config::instance();
    switch (cfg.preferredPlayer()) {
    case core::player::Kind::Embedded:
        // Persisted choice survives even if the binary is now
        // compiled without libmpv — keep it selected so rebuilding
        // with libmpv restores the user's preference. If the radio
        // is disabled, the user must pick another option before
        // Apply lets a non-embedded value take over.
        m_embeddedRadio->setChecked(true);
        break;
    case core::player::Kind::Vlc:
        m_vlcRadio->setChecked(true);
        break;
    case core::player::Kind::Custom:
        m_customRadio->setChecked(true);
        break;
    case core::player::Kind::Mpv:
    default:
        m_mpvRadio->setChecked(true);
        break;
    }
    m_customCmdEdit->setText(cfg.customPlayerCommand());
    updateCustomEnabled();
}

void PlayerSettingsPage::apply()
{
    auto& cfg = config::Config::instance();
    core::player::Kind chosen = core::player::Kind::Mpv;
    if (m_embeddedRadio->isChecked()) {
        chosen = core::player::Kind::Embedded;
    } else if (m_vlcRadio->isChecked()) {
        chosen = core::player::Kind::Vlc;
    } else if (m_customRadio->isChecked()) {
        chosen = core::player::Kind::Custom;
    }
    cfg.setPreferredPlayer(chosen);
    cfg.setCustomPlayerCommand(m_customCmdEdit->text());
}

void PlayerSettingsPage::resetToDefaults()
{
    m_mpvRadio->setChecked(true);
    m_customCmdEdit->clear();
    updateCustomEnabled();
}

void PlayerSettingsPage::updateCustomEnabled()
{
    m_customCmdEdit->setEnabled(m_customRadio->isChecked());
}

} // namespace kinema::ui::settings
