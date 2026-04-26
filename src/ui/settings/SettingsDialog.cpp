// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/settings/SettingsDialog.h"

#include "config/AppSettings.h"
#include "ui/settings/FiltersSettingsPage.h"
#include "ui/settings/GeneralSettingsPage.h"
#include "ui/settings/PlayerSettingsPage.h"
#include "ui/settings/RealDebridSettingsPage.h"
#include "ui/settings/SubtitlesSettingsPage.h"
#include "ui/settings/TmdbSettingsPage.h"

#include <KLocalizedString>

#include <KPageWidgetItem>

#include <QDialogButtonBox>
#include <QIcon>
#include <QPushButton>

namespace kinema::ui::settings {

SettingsDialog::SettingsDialog(
    core::HttpClient* http, core::TokenStore* tokens,
    config::AppSettings& settings,
    core::SubtitleCacheStore* subtitleCache,
    QWidget* parent)
    : KPageDialog(parent)
{
    setWindowTitle(i18nc("@title:window", "Configure Kinema"));
    setFaceType(KPageDialog::List);
    resize(820, 560);

    m_generalPage = new GeneralSettingsPage(
        settings.search(), settings.torrentio(),
        settings.appearance(), this);
    m_filtersPage = new FiltersSettingsPage(settings.filter(), this);
    m_playerPage = new PlayerSettingsPage(settings.player(), this);
    m_rdPage = new RealDebridSettingsPage(
        http, tokens, settings.realDebrid(), this);
    m_tmdbPage = new TmdbSettingsPage(http, tokens, this);
    m_subsPage = new SubtitlesSettingsPage(http, tokens,
        settings.subtitle(), settings.cache(), subtitleCache, this);

    auto* generalItem = addPage(m_generalPage,
        i18nc("@title:tab settings page", "General"));
    generalItem->setIcon(QIcon::fromTheme(QStringLiteral("preferences-other")));

    auto* filtersItem = addPage(m_filtersPage,
        i18nc("@title:tab settings page", "Filters"));
    filtersItem->setIcon(QIcon::fromTheme(QStringLiteral("view-filter")));

    auto* playerItem = addPage(m_playerPage,
        i18nc("@title:tab settings page", "Player"));
    playerItem->setIcon(QIcon::fromTheme(QStringLiteral("media-playback-start")));

    auto* rdItem = addPage(m_rdPage,
        i18nc("@title:tab settings page", "Real-Debrid"));
    rdItem->setIcon(QIcon::fromTheme(QStringLiteral("network-server")));

    auto* tmdbItem = addPage(m_tmdbPage,
        i18nc("@title:tab settings page", "TMDB (Discover)"));
    tmdbItem->setIcon(QIcon::fromTheme(QStringLiteral("applications-multimedia")));

    auto* subsItem = addPage(m_subsPage,
        i18nc("@title:tab settings page", "Subtitles"));
    subsItem->setIcon(QIcon::fromTheme(QStringLiteral("media-view-subtitles-symbolic"),
        QIcon::fromTheme(QStringLiteral("text-x-generic"))));

    setStandardButtons(QDialogButtonBox::Ok | QDialogButtonBox::Apply
        | QDialogButtonBox::Cancel | QDialogButtonBox::RestoreDefaults);

    if (auto* applyBtn = button(QDialogButtonBox::Apply)) {
        connect(applyBtn, &QPushButton::clicked,
            this, &SettingsDialog::applyAll);
    }
    if (auto* resetBtn = button(QDialogButtonBox::RestoreDefaults)) {
        connect(resetBtn, &QPushButton::clicked,
            this, &SettingsDialog::resetAllToDefaults);
    }

    // OK = apply + accept. The QDialog Accepted signal fires after
    // accept() is called; applyAll() must run first.
    connect(this, &QDialog::accepted, this, &SettingsDialog::applyAll);

    // Forward RD token changes to MainWindow.
    connect(m_rdPage, &RealDebridSettingsPage::tokenChanged,
        this, &SettingsDialog::tokenChanged);

    // Forward TMDB token changes to MainWindow (distinct signal so
    // the two keyring slots don't get crossed).
    connect(m_tmdbPage, &TmdbSettingsPage::tokenChanged,
        this, &SettingsDialog::tmdbTokenChanged);

    // Forward subtitle credential changes so MainWindow can refresh
    // TokenController.
    connect(m_subsPage, &SubtitlesSettingsPage::tokenChanged,
        this, &SettingsDialog::subtitleCredentialsChanged);
}

void SettingsDialog::applyAll()
{
    m_generalPage->apply();
    m_filtersPage->apply();
    m_playerPage->apply();
    if (m_subsPage) {
        m_subsPage->apply();
    }
    // RD and TMDB pages are self-managed (Save/Remove buttons inside
    // the page).
}

void SettingsDialog::resetAllToDefaults()
{
    m_generalPage->resetToDefaults();
    m_filtersPage->resetToDefaults();
    m_playerPage->resetToDefaults();
    if (m_subsPage) {
        m_subsPage->resetToDefaults();
    }
    // RD / TMDB / Subtitles credentials pages: we don't auto-wipe
    // tokens here. Removing credentials is destructive and requires
    // explicit user intent (Remove button).
}

} // namespace kinema::ui::settings
