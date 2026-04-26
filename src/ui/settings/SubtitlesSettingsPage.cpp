// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/settings/SubtitlesSettingsPage.h"

#include "api/OpenSubtitlesClient.h"
#include "api/Subtitle.h"
#include "config/CacheSettings.h"
#include "config/SubtitleSettings.h"
#include "core/HttpClient.h"
#include "core/HttpError.h"
#include "core/HttpErrorPresenter.h"
#include "core/Language.h"
#include "core/SubtitleCacheStore.h"
#include "core/TokenStore.h"
#include "kinema_debug.h"

#include <KColorScheme>
#include <KLocalizedString>

#include <QCheckBox>
#include <QComboBox>
#include <QFile>
#include <QFormLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QIcon>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QMessageBox>
#include <QPushButton>
#include <QSpinBox>
#include <QVBoxLayout>

namespace kinema::ui::settings {

namespace {

void populateModeCombo(QComboBox* combo)
{
    combo->addItem(i18nc("@item:inlistbox subtitle filter mode",
                       "Off"),
        QStringLiteral("off"));
    combo->addItem(i18nc("@item:inlistbox subtitle filter mode",
                       "Include"),
        QStringLiteral("include"));
    combo->addItem(i18nc("@item:inlistbox subtitle filter mode",
                       "Only"),
        QStringLiteral("only"));
}

void selectMode(QComboBox* combo, const QString& mode)
{
    for (int i = 0; i < combo->count(); ++i) {
        if (combo->itemData(i).toString() == mode) {
            combo->setCurrentIndex(i);
            return;
        }
    }
    combo->setCurrentIndex(0);
}

} // namespace

SubtitlesSettingsPage::SubtitlesSettingsPage(core::HttpClient* http,
    core::TokenStore* tokens,
    config::SubtitleSettings& subtitleSettings,
    config::CacheSettings& cacheSettings,
    core::SubtitleCacheStore* subtitleCache,
    QWidget* parent)
    : QWidget(parent)
    , m_http(http)
    , m_tokens(tokens)
    , m_subtitleSettings(subtitleSettings)
    , m_cacheSettings(cacheSettings)
    , m_subtitleCache(subtitleCache)
{
    auto* intro = new QLabel(
        i18nc("@info",
            "Sign in with an OpenSubtitles.com account to download subtitles "
            "from inside the player. Searches send the IMDb id of the title "
            "to opensubtitles.com. Get an API key at "
            "<a href=\"https://www.opensubtitles.com/consumers\">"
            "opensubtitles.com/consumers</a>."),
        this);
    intro->setWordWrap(true);
    intro->setOpenExternalLinks(true);
    intro->setTextFormat(Qt::RichText);

    // ---- Credentials group --------------------------------------------
    auto* credBox = new QGroupBox(
        i18nc("@title:group", "OpenSubtitles account"), this);
    m_apiKeyEdit = new QLineEdit(credBox);
    m_apiKeyEdit->setEchoMode(QLineEdit::Password);
    m_apiKeyEdit->setPlaceholderText(
        i18nc("@info:placeholder", "API key"));
    m_usernameEdit = new QLineEdit(credBox);
    m_usernameEdit->setPlaceholderText(
        i18nc("@info:placeholder", "Username"));
    m_passwordEdit = new QLineEdit(credBox);
    m_passwordEdit->setEchoMode(QLineEdit::Password);
    m_passwordEdit->setPlaceholderText(
        i18nc("@info:placeholder", "Password"));

    m_testButton = new QPushButton(
        QIcon::fromTheme(QStringLiteral("network-connect")),
        i18nc("@action:button", "Test connection"), credBox);
    m_saveButton = new QPushButton(
        QIcon::fromTheme(QStringLiteral("document-save")),
        i18nc("@action:button", "Save credentials"), credBox);
    m_removeButton = new QPushButton(
        QIcon::fromTheme(QStringLiteral("edit-delete")),
        i18nc("@action:button", "Remove credentials"), credBox);

    m_statusLabel = new QLabel(credBox);
    m_statusLabel->setWordWrap(true);
    m_statusLabel->setMinimumHeight(48);

    auto* form = new QFormLayout;
    form->addRow(i18nc("@label:textbox", "API key:"), m_apiKeyEdit);
    form->addRow(i18nc("@label:textbox", "Username:"), m_usernameEdit);
    form->addRow(i18nc("@label:textbox", "Password:"), m_passwordEdit);

    auto* actionRow = new QHBoxLayout;
    actionRow->addWidget(m_testButton);
    actionRow->addWidget(m_saveButton);
    actionRow->addStretch(1);
    actionRow->addWidget(m_removeButton);

    auto* credLayout = new QVBoxLayout(credBox);
    credLayout->addLayout(form);
    credLayout->addLayout(actionRow);
    credLayout->addWidget(m_statusLabel);

    // ---- Preferred languages -----------------------------------------
    auto* langBox = new QGroupBox(
        i18nc("@title:group", "Preferred languages"), this);
    m_langList = new QListWidget(langBox);
    m_langList->setMinimumHeight(120);

    m_addLangCombo = new QComboBox(langBox);
    for (const auto& opt : core::language::commonLanguages()) {
        m_addLangCombo->addItem(
            QStringLiteral("%1 (%2)").arg(opt.display, opt.code), opt.code);
    }

    m_addLangButton = new QPushButton(
        QIcon::fromTheme(QStringLiteral("list-add")),
        i18nc("@action:button", "Add"), langBox);
    m_removeLangButton = new QPushButton(
        QIcon::fromTheme(QStringLiteral("list-remove")),
        i18nc("@action:button", "Remove"), langBox);
    m_upLangButton = new QPushButton(
        QIcon::fromTheme(QStringLiteral("go-up")),
        i18nc("@action:button", "Up"), langBox);
    m_downLangButton = new QPushButton(
        QIcon::fromTheme(QStringLiteral("go-down")),
        i18nc("@action:button", "Down"), langBox);

    auto* langActions = new QHBoxLayout;
    langActions->addWidget(m_addLangCombo, 1);
    langActions->addWidget(m_addLangButton);
    langActions->addWidget(m_removeLangButton);
    langActions->addWidget(m_upLangButton);
    langActions->addWidget(m_downLangButton);

    auto* langLayout = new QVBoxLayout(langBox);
    langLayout->addWidget(m_langList);
    langLayout->addLayout(langActions);

    // ---- Filters defaults --------------------------------------------
    auto* filterBox = new QGroupBox(
        i18nc("@title:group", "Default filters"), this);
    m_hiCombo = new QComboBox(filterBox);
    populateModeCombo(m_hiCombo);
    m_fpoCombo = new QComboBox(filterBox);
    populateModeCombo(m_fpoCombo);

    auto* filterForm = new QFormLayout(filterBox);
    filterForm->addRow(i18nc("@label", "Hearing impaired:"), m_hiCombo);
    filterForm->addRow(i18nc("@label", "Foreign parts only:"), m_fpoCombo);

    // ---- Cache --------------------------------------------------------
    auto* cacheBox = new QGroupBox(
        i18nc("@title:group", "Cache"), this);
    m_cacheBudgetSpin = new QSpinBox(cacheBox);
    m_cacheBudgetSpin->setRange(1, 10000);
    m_cacheBudgetSpin->setSuffix(QStringLiteral(" MB"));
    m_clearCacheButton = new QPushButton(
        QIcon::fromTheme(QStringLiteral("edit-clear")),
        i18nc("@action:button", "Clear subtitle cache"), cacheBox);

    auto* cacheRow = new QHBoxLayout;
    cacheRow->addWidget(new QLabel(
        i18nc("@label", "Disk budget:"), cacheBox));
    cacheRow->addWidget(m_cacheBudgetSpin);
    cacheRow->addStretch(1);
    cacheRow->addWidget(m_clearCacheButton);

    auto* cacheLayout = new QVBoxLayout(cacheBox);
    cacheLayout->addLayout(cacheRow);

    // ---- Page layout --------------------------------------------------
    auto* root = new QVBoxLayout(this);
    root->addWidget(intro);
    root->addWidget(credBox);
    root->addWidget(langBox);
    root->addWidget(filterBox);
    root->addWidget(cacheBox);
    root->addStretch(1);

    // ---- Connections --------------------------------------------------
    connect(m_apiKeyEdit, &QLineEdit::textChanged,
        this, [this] { updateButtons(); });
    connect(m_usernameEdit, &QLineEdit::textChanged,
        this, [this] { updateButtons(); });
    connect(m_passwordEdit, &QLineEdit::textChanged,
        this, [this] { updateButtons(); });
    connect(m_testButton, &QPushButton::clicked, this, [this] {
        auto t = testConnection();
        Q_UNUSED(t);
    });
    connect(m_saveButton, &QPushButton::clicked, this, [this] {
        auto t = saveCredentials();
        Q_UNUSED(t);
    });
    connect(m_removeButton, &QPushButton::clicked, this, [this] {
        auto t = removeCredentials();
        Q_UNUSED(t);
    });
    connect(m_addLangButton, &QPushButton::clicked,
        this, &SubtitlesSettingsPage::addLanguageRow);
    connect(m_removeLangButton, &QPushButton::clicked,
        this, &SubtitlesSettingsPage::removeSelectedLanguage);
    connect(m_upLangButton, &QPushButton::clicked,
        this, &SubtitlesSettingsPage::moveLanguageUp);
    connect(m_downLangButton, &QPushButton::clicked,
        this, &SubtitlesSettingsPage::moveLanguageDown);
    connect(m_clearCacheButton, &QPushButton::clicked,
        this, &SubtitlesSettingsPage::onClearCache);

    // ---- Initial population from settings -----------------------------
    for (const auto& code : m_subtitleSettings.preferredLanguages()) {
        auto* item = new QListWidgetItem(
            QStringLiteral("%1 (%2)")
                .arg(core::language::displayName(code), code));
        item->setData(Qt::UserRole, code);
        m_langList->addItem(item);
    }
    selectMode(m_hiCombo, m_subtitleSettings.hearingImpaired());
    selectMode(m_fpoCombo, m_subtitleSettings.foreignPartsOnly());
    m_cacheBudgetSpin->setValue(m_cacheSettings.subtitleBudgetMb());

    updateButtons();

    auto t = loadExistingTokens();
    Q_UNUSED(t);
}

void SubtitlesSettingsPage::apply()
{
    QStringList langs;
    for (int i = 0; i < m_langList->count(); ++i) {
        langs << m_langList->item(i)->data(Qt::UserRole).toString();
    }
    m_subtitleSettings.setPreferredLanguages(langs);
    m_subtitleSettings.setHearingImpaired(
        m_hiCombo->currentData().toString());
    m_subtitleSettings.setForeignPartsOnly(
        m_fpoCombo->currentData().toString());
    m_cacheSettings.setSubtitleBudgetMb(m_cacheBudgetSpin->value());
}

void SubtitlesSettingsPage::resetToDefaults()
{
    m_langList->clear();
    selectMode(m_hiCombo, QStringLiteral("off"));
    selectMode(m_fpoCombo, QStringLiteral("off"));
    m_cacheBudgetSpin->setValue(200);
}

void SubtitlesSettingsPage::updateButtons()
{
    const bool hasAll = !m_apiKeyEdit->text().trimmed().isEmpty()
        && !m_usernameEdit->text().trimmed().isEmpty()
        && !m_passwordEdit->text().trimmed().isEmpty();
    m_testButton->setEnabled(hasAll);
    m_saveButton->setEnabled(hasAll);
    m_removeButton->setEnabled(m_credentialsSaved);
}

void SubtitlesSettingsPage::setStatus(const QString& message, bool error)
{
    m_statusLabel->setText(message);
    const KColorScheme scheme(QPalette::Active, KColorScheme::Window);
    const auto role = error
        ? KColorScheme::NegativeText
        : KColorScheme::PositiveText;
    const auto colour = scheme.foreground(role).color().name();
    m_statusLabel->setStyleSheet(
        QStringLiteral("color: %1; font-weight: 500;").arg(colour));
}

void SubtitlesSettingsPage::clearStatus()
{
    m_statusLabel->clear();
    m_statusLabel->setStyleSheet(QString {});
}

QCoro::Task<void> SubtitlesSettingsPage::loadExistingTokens()
{
    try {
        const auto apiKey = co_await m_tokens->read(
            QString::fromLatin1(core::TokenStore::kOpenSubtitlesApiKey));
        const auto username = co_await m_tokens->read(
            QString::fromLatin1(core::TokenStore::kOpenSubtitlesUsername));
        const auto password = co_await m_tokens->read(
            QString::fromLatin1(core::TokenStore::kOpenSubtitlesPassword));
        if (!apiKey.isEmpty()) {
            m_apiKeyEdit->setText(apiKey);
        }
        if (!username.isEmpty()) {
            m_usernameEdit->setText(username);
        }
        if (!password.isEmpty()) {
            m_passwordEdit->setText(password);
        }
        m_credentialsSaved = !apiKey.isEmpty()
            && !username.isEmpty() && !password.isEmpty();
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), /*error=*/true);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "subtitles settings/load"),
            /*error=*/true);
    }
    updateButtons();
}

QCoro::Task<void> SubtitlesSettingsPage::testConnection()
{
    const auto apiKey = m_apiKeyEdit->text().trimmed();
    const auto username = m_usernameEdit->text().trimmed();
    const auto password = m_passwordEdit->text();
    if (apiKey.isEmpty() || username.isEmpty() || password.isEmpty()) {
        co_return;
    }
    clearStatus();
    m_testButton->setEnabled(false);

    api::OpenSubtitlesClient client(m_http, apiKey, username, password);
    try {
        co_await client.ensureLoggedIn();

        api::SubtitleSearchQuery q;
        q.key.kind = api::MediaKind::Movie;
        q.key.imdbId = QStringLiteral("tt0133093");
        const auto hits = co_await client.search(q);
        setStatus(i18nc("@info subtitles connection probe",
                      "Connected · %1 results found.", hits.size()),
            /*error=*/false);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "subtitles settings/test"),
            /*error=*/true);
    }
    updateButtons();
}

QCoro::Task<void> SubtitlesSettingsPage::saveCredentials()
{
    const auto apiKey = m_apiKeyEdit->text().trimmed();
    const auto username = m_usernameEdit->text().trimmed();
    const auto password = m_passwordEdit->text();
    if (apiKey.isEmpty() || username.isEmpty() || password.isEmpty()) {
        co_return;
    }
    m_saveButton->setEnabled(false);
    try {
        co_await m_tokens->write(
            QString::fromLatin1(core::TokenStore::kOpenSubtitlesApiKey),
            apiKey);
        co_await m_tokens->write(
            QString::fromLatin1(core::TokenStore::kOpenSubtitlesUsername),
            username);
        co_await m_tokens->write(
            QString::fromLatin1(core::TokenStore::kOpenSubtitlesPassword),
            password);
        m_credentialsSaved = true;
        Q_EMIT tokenChanged();
        setStatus(i18nc("@info", "Credentials saved to keyring."),
            /*error=*/false);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), /*error=*/true);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "subtitles settings/save"),
            /*error=*/true);
    }
    updateButtons();
}

QCoro::Task<void> SubtitlesSettingsPage::removeCredentials()
{
    m_removeButton->setEnabled(false);
    try {
        co_await m_tokens->remove(
            QString::fromLatin1(core::TokenStore::kOpenSubtitlesApiKey));
        co_await m_tokens->remove(
            QString::fromLatin1(core::TokenStore::kOpenSubtitlesUsername));
        co_await m_tokens->remove(
            QString::fromLatin1(core::TokenStore::kOpenSubtitlesPassword));
        m_apiKeyEdit->clear();
        m_usernameEdit->clear();
        m_passwordEdit->clear();
        m_credentialsSaved = false;
        Q_EMIT tokenChanged();
        setStatus(i18nc("@info", "Credentials removed from keyring."),
            /*error=*/false);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), /*error=*/true);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "subtitles settings/remove"),
            /*error=*/true);
    }
    updateButtons();
}

void SubtitlesSettingsPage::addLanguageRow()
{
    const auto code = m_addLangCombo->currentData().toString();
    if (code.isEmpty()) {
        return;
    }
    // Skip if already in the list.
    for (int i = 0; i < m_langList->count(); ++i) {
        if (m_langList->item(i)->data(Qt::UserRole).toString() == code) {
            return;
        }
    }
    auto* item = new QListWidgetItem(
        QStringLiteral("%1 (%2)")
            .arg(core::language::displayName(code), code));
    item->setData(Qt::UserRole, code);
    m_langList->addItem(item);
}

void SubtitlesSettingsPage::removeSelectedLanguage()
{
    const auto items = m_langList->selectedItems();
    for (auto* item : items) {
        delete m_langList->takeItem(m_langList->row(item));
    }
}

void SubtitlesSettingsPage::moveLanguageUp()
{
    const int row = m_langList->currentRow();
    if (row <= 0) {
        return;
    }
    auto* item = m_langList->takeItem(row);
    m_langList->insertItem(row - 1, item);
    m_langList->setCurrentRow(row - 1);
}

void SubtitlesSettingsPage::moveLanguageDown()
{
    const int row = m_langList->currentRow();
    if (row < 0 || row >= m_langList->count() - 1) {
        return;
    }
    auto* item = m_langList->takeItem(row);
    m_langList->insertItem(row + 1, item);
    m_langList->setCurrentRow(row + 1);
}

void SubtitlesSettingsPage::onClearCache()
{
    if (!m_subtitleCache) {
        return;
    }
    const auto ret = QMessageBox::question(this,
        i18nc("@title:window", "Clear subtitle cache"),
        i18nc("@info",
            "Remove every cached subtitle from disk?\n"
            "Your OpenSubtitles credentials are not affected."),
        QMessageBox::Yes | QMessageBox::No, QMessageBox::No);
    if (ret != QMessageBox::Yes) {
        return;
    }

    // Delete every cached file under <CacheLocation>/subtitles/, then
    // truncate the table.
    const auto entries = m_subtitleCache->all();
    for (const auto& e : entries) {
        if (!e.localPath.isEmpty()) {
            QFile::remove(e.localPath);
        }
    }
    m_subtitleCache->clearAll();
    setStatus(i18nc("@info", "Subtitle cache cleared (%1 file(s)).",
                  entries.size()),
        /*error=*/false);
}

} // namespace kinema::ui::settings
