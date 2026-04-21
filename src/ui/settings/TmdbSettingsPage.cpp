// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/settings/TmdbSettingsPage.h"

#include "api/TmdbClient.h"
#include "core/HttpClient.h"
#include "core/HttpError.h"
#include "core/TmdbConfig.h"
#include "core/TokenStore.h"
#include "kinema_debug.h"

#include <KColorScheme>
#include <KLocalizedString>

#include <QFormLayout>
#include <QHBoxLayout>
#include <QIcon>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QString>
#include <QVBoxLayout>

namespace kinema::ui::settings {

namespace {

bool compiledDefaultPresent()
{
    return core::kTmdbCompiledDefaultToken != nullptr
        && core::kTmdbCompiledDefaultToken[0] != '\0';
}

} // namespace

TmdbSettingsPage::TmdbSettingsPage(
    core::HttpClient* http, core::TokenStore* tokens, QWidget* parent)
    : QWidget(parent)
    , m_http(http)
    , m_tokens(tokens)
{
    auto* intro = new QLabel(
        i18nc("@info",
            "Discover uses a shared TMDB read-access token that ships "
            "with Kinema, so it works out of the box. Paste your own "
            "v4 token below only if you want to use your own account "
            "— for example, to avoid sharing a rate-limit pool, or as "
            "a fallback if the bundled token has been revoked. Get a "
            "token at <a href=\"https://www.themoviedb.org/settings/api\">"
            "themoviedb.org/settings/api</a>. Kinema stores it in your "
            "system keyring — never on disk in plaintext."),
        this);
    intro->setWordWrap(true);
    intro->setOpenExternalLinks(true);
    intro->setTextFormat(Qt::RichText);

    m_tokenEdit = new QLineEdit(this);
    m_tokenEdit->setEchoMode(QLineEdit::Password);
    m_tokenEdit->setPlaceholderText(
        i18nc("@info:placeholder",
            "Leave empty to use the bundled token"));

    m_showHideButton = new QPushButton(this);
    m_showHideButton->setIcon(QIcon::fromTheme(QStringLiteral("view-visible")));
    m_showHideButton->setToolTip(i18nc("@info:tooltip", "Show / hide token"));
    m_showHideButton->setCheckable(true);
    m_showHideButton->setFixedWidth(36);
    connect(m_showHideButton, &QPushButton::toggled, this, [this](bool on) {
        m_tokenEdit->setEchoMode(on ? QLineEdit::Normal : QLineEdit::Password);
    });

    auto* tokenRow = new QHBoxLayout;
    tokenRow->setContentsMargins(0, 0, 0, 0);
    tokenRow->addWidget(m_tokenEdit, 1);
    tokenRow->addWidget(m_showHideButton);

    m_testButton = new QPushButton(
        QIcon::fromTheme(QStringLiteral("network-connect")),
        i18nc("@action:button", "Test connection"), this);
    m_saveButton = new QPushButton(
        QIcon::fromTheme(QStringLiteral("document-save")),
        i18nc("@action:button", "Save token"), this);
    m_removeButton = new QPushButton(
        QIcon::fromTheme(QStringLiteral("edit-delete")),
        i18nc("@action:button", "Remove token"), this);

    m_statusLabel = new QLabel(this);
    m_statusLabel->setWordWrap(true);
    m_statusLabel->setTextFormat(Qt::PlainText);
    m_statusLabel->setMinimumHeight(48);

    auto* form = new QFormLayout;
    form->addRow(i18nc("@label:textbox", "v4 Read Access Token:"), tokenRow);

    auto* actionRow = new QHBoxLayout;
    actionRow->addWidget(m_testButton);
    actionRow->addWidget(m_saveButton);
    actionRow->addStretch(1);
    actionRow->addWidget(m_removeButton);

    auto* layout = new QVBoxLayout(this);
    layout->addWidget(intro);
    layout->addLayout(form);
    layout->addLayout(actionRow);
    layout->addWidget(m_statusLabel);
    layout->addStretch(1);

    connect(m_testButton, &QPushButton::clicked, this, [this] {
        auto t = testConnection();
        Q_UNUSED(t);
    });
    connect(m_saveButton, &QPushButton::clicked, this, [this] {
        auto t = saveToken();
        Q_UNUSED(t);
    });
    connect(m_removeButton, &QPushButton::clicked, this, [this] {
        auto t = removeToken();
        Q_UNUSED(t);
    });
    connect(m_tokenEdit, &QLineEdit::textChanged, this, [this] { updateButtons(); });

    updateButtons();

    auto t = loadExistingToken();
    Q_UNUSED(t);
}

QString TmdbSettingsPage::effectiveTokenSummary(bool userTokenPresent) const
{
    if (userTokenPresent) {
        return i18nc("@info", "Using your token.");
    }
    if (compiledDefaultPresent()) {
        return i18nc("@info", "Using the bundled token.");
    }
    return i18nc("@info",
        "No token configured. Discover is disabled. Set one above to enable it.");
}

void TmdbSettingsPage::updateButtons()
{
    const bool hasText = !m_tokenEdit->text().trimmed().isEmpty();
    m_testButton->setEnabled(hasText || m_userTokenSaved
        || compiledDefaultPresent());
    m_saveButton->setEnabled(hasText);
    m_removeButton->setEnabled(m_userTokenSaved);
}

void TmdbSettingsPage::setStatus(const QString& message, bool error)
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

void TmdbSettingsPage::clearStatus()
{
    m_statusLabel->clear();
    m_statusLabel->setStyleSheet(QString {});
}

QCoro::Task<void> TmdbSettingsPage::loadExistingToken()
{
    try {
        const auto existing = co_await m_tokens->read(
            QString::fromLatin1(core::TokenStore::kTmdbKey));
        if (!existing.isEmpty()) {
            m_tokenEdit->setText(existing);
            m_userTokenSaved = true;
        } else {
            m_userTokenSaved = false;
        }
        setStatus(effectiveTokenSummary(m_userTokenSaved), /*error=*/false);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), /*error=*/true);
    } catch (const std::exception& e) {
        qCWarning(KINEMA) << "TMDB settings: keyring read failed:" << e.what();
        setStatus(QString::fromUtf8(e.what()), /*error=*/true);
    }
    updateButtons();
}

QCoro::Task<void> TmdbSettingsPage::testConnection()
{
    // Prefer the text currently in the edit field; if empty, fall back
    // to the effective token (user saved → compile-time default).
    QString token = m_tokenEdit->text().trimmed();
    if (token.isEmpty()) {
        if (m_userTokenSaved) {
            try {
                token = co_await m_tokens->read(
                    QString::fromLatin1(core::TokenStore::kTmdbKey));
            } catch (const std::exception& e) {
                setStatus(QString::fromUtf8(e.what()), /*error=*/true);
                co_return;
            }
        }
        if (token.isEmpty() && compiledDefaultPresent()) {
            token = QString::fromLatin1(core::kTmdbCompiledDefaultToken);
        }
    }
    if (token.isEmpty()) {
        setStatus(i18nc("@info",
            "No token to test. Paste one above or build with "
            "KINEMA_TMDB_DEFAULT_TOKEN."),
            /*error=*/true);
        co_return;
    }

    clearStatus();
    m_testButton->setEnabled(false);

    api::TmdbClient client(m_http);
    client.setToken(token);

    try {
        co_await client.testAuth();
        setStatus(i18nc("@info", "Connected to TMDB."), /*error=*/false);
    } catch (const core::HttpError& e) {
        if (e.httpStatus() == 401 || e.httpStatus() == 403) {
            setStatus(
                i18nc("@info", "TMDB rejected the token (HTTP %1).",
                    e.httpStatus()),
                /*error=*/true);
        } else {
            setStatus(e.message(), /*error=*/true);
        }
    } catch (const std::exception& e) {
        setStatus(QString::fromUtf8(e.what()), /*error=*/true);
    }

    updateButtons();
}

QCoro::Task<void> TmdbSettingsPage::saveToken()
{
    const auto token = m_tokenEdit->text().trimmed();
    if (token.isEmpty()) {
        co_return;
    }
    m_saveButton->setEnabled(false);
    try {
        co_await m_tokens->write(
            QString::fromLatin1(core::TokenStore::kTmdbKey), token);
        m_userTokenSaved = true;
        Q_EMIT tokenChanged(token);
        setStatus(i18nc("@info", "Token saved to keyring. Using your token."),
            /*error=*/false);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), /*error=*/true);
    } catch (const std::exception& e) {
        setStatus(QString::fromUtf8(e.what()), /*error=*/true);
    }
    updateButtons();
}

QCoro::Task<void> TmdbSettingsPage::removeToken()
{
    m_removeButton->setEnabled(false);
    try {
        co_await m_tokens->remove(
            QString::fromLatin1(core::TokenStore::kTmdbKey));
        m_userTokenSaved = false;
        m_tokenEdit->clear();
        Q_EMIT tokenChanged(QString {});
        setStatus(effectiveTokenSummary(/*userTokenPresent=*/false),
            /*error=*/false);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), /*error=*/true);
    } catch (const std::exception& e) {
        setStatus(QString::fromUtf8(e.what()), /*error=*/true);
    }
    updateButtons();
}

} // namespace kinema::ui::settings
