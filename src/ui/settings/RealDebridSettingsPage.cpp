// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/settings/RealDebridSettingsPage.h"

#include "api/RealDebridClient.h"
#include "config/Config.h"
#include "core/DateFormat.h"
#include "core/HttpClient.h"
#include "core/HttpError.h"
#include "core/TokenStore.h"
#include "kinema_debug.h"

#include <KLocalizedString>

#include <QFormLayout>
#include <QHBoxLayout>
#include <QIcon>
#include <QLabel>
#include <QLineEdit>
#include <KColorScheme>

#include <QPushButton>
#include <QVBoxLayout>

namespace kinema::ui::settings {

RealDebridSettingsPage::RealDebridSettingsPage(
    core::HttpClient* http, core::TokenStore* tokens, QWidget* parent)
    : QWidget(parent)
    , m_http(http)
    , m_tokens(tokens)
{
    auto* intro = new QLabel(
        i18nc("@info",
            "Paste your Real-Debrid API token below. You can find it in "
            "your account page at <a href=\"https://real-debrid.com/apitoken\">"
            "real-debrid.com/apitoken</a>. Kinema stores the token in your "
            "system keyring — never on disk in plaintext."),
        this);
    intro->setWordWrap(true);
    intro->setOpenExternalLinks(true);
    intro->setTextFormat(Qt::RichText);

    m_tokenEdit = new QLineEdit(this);
    m_tokenEdit->setEchoMode(QLineEdit::Password);
    m_tokenEdit->setPlaceholderText(
        i18nc("@info:placeholder", "Real-Debrid API token"));

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
    form->addRow(i18nc("@label:textbox", "API token:"), tokenRow);

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

void RealDebridSettingsPage::updateButtons()
{
    const bool hasText = !m_tokenEdit->text().trimmed().isEmpty();
    m_testButton->setEnabled(hasText);
    m_saveButton->setEnabled(hasText);
    m_removeButton->setEnabled(config::Config::instance().hasRealDebrid());
}

void RealDebridSettingsPage::setStatus(const QString& message, bool error)
{
    m_statusLabel->setText(message);
    // Pull the semantic positive/negative foreground from the active
    // KColorScheme so the status reads correctly on both light and
    // dark themes. Hardcoded hex colours (the previous approach) were
    // too dark to see on dark Plasma palettes.
    const KColorScheme scheme(QPalette::Active, KColorScheme::Window);
    const auto role = error
        ? KColorScheme::NegativeText
        : KColorScheme::PositiveText;
    const auto colour = scheme.foreground(role).color().name();
    m_statusLabel->setStyleSheet(
        QStringLiteral("color: %1; font-weight: 500;").arg(colour));
}

void RealDebridSettingsPage::clearStatus()
{
    m_statusLabel->clear();
    m_statusLabel->setStyleSheet(QString {});
}

QCoro::Task<void> RealDebridSettingsPage::loadExistingToken()
{
    try {
        const auto existing = co_await m_tokens->read(
            QString::fromLatin1(core::TokenStore::kRealDebridKey));
        if (!existing.isEmpty()) {
            m_tokenEdit->setText(existing);
        }
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), /*error=*/true);
    } catch (const std::exception& e) {
        qCWarning(KINEMA) << "RD settings: keyring read failed:" << e.what();
        setStatus(QString::fromUtf8(e.what()), /*error=*/true);
    }
    updateButtons();
}

QCoro::Task<void> RealDebridSettingsPage::testConnection()
{
    const auto token = m_tokenEdit->text().trimmed();
    if (token.isEmpty()) {
        co_return;
    }
    clearStatus();
    m_testButton->setEnabled(false);

    api::RealDebridClient client(m_http);
    client.setToken(token);

    try {
        const auto user = co_await client.user();

        QString msg = i18nc("@info",
            "Signed in as %1 (%2)",
            user.username.isEmpty() ? QStringLiteral("—") : user.username,
            user.type.isEmpty() ? QStringLiteral("?") : user.type);
        if (user.premiumUntil) {
            msg += QLatin1Char('\n')
                + i18nc("@info", "Premium until: %1",
                    core::formatReleaseDate(*user.premiumUntil));
        }
        setStatus(msg, /*error=*/false);
    } catch (const core::HttpError& e) {
        if (e.httpStatus() == 401 || e.httpStatus() == 403) {
            setStatus(
                i18nc("@info", "Real-Debrid rejected the token (HTTP %1).",
                    e.httpStatus()),
                /*error=*/true);
        } else {
            setStatus(e.message(), /*error=*/true);
        }
    } catch (const std::exception& e) {
        setStatus(QString::fromUtf8(e.what()), /*error=*/true);
    }

    m_testButton->setEnabled(!m_tokenEdit->text().trimmed().isEmpty());
}

QCoro::Task<void> RealDebridSettingsPage::saveToken()
{
    const auto token = m_tokenEdit->text().trimmed();
    if (token.isEmpty()) {
        co_return;
    }
    m_saveButton->setEnabled(false);
    try {
        co_await m_tokens->write(
            QString::fromLatin1(core::TokenStore::kRealDebridKey), token);
        config::Config::instance().setHasRealDebrid(true);
        Q_EMIT tokenChanged(token);
        setStatus(i18nc("@info", "Token saved to keyring."), /*error=*/false);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), /*error=*/true);
    } catch (const std::exception& e) {
        setStatus(QString::fromUtf8(e.what()), /*error=*/true);
    }
    updateButtons();
}

QCoro::Task<void> RealDebridSettingsPage::removeToken()
{
    m_removeButton->setEnabled(false);
    try {
        co_await m_tokens->remove(
            QString::fromLatin1(core::TokenStore::kRealDebridKey));
        config::Config::instance().setHasRealDebrid(false);
        m_tokenEdit->clear();
        Q_EMIT tokenChanged(QString {});
        setStatus(i18nc("@info", "Token removed from keyring."),
            /*error=*/false);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), /*error=*/true);
    } catch (const std::exception& e) {
        setStatus(QString::fromUtf8(e.what()), /*error=*/true);
    }
    updateButtons();
}

} // namespace kinema::ui::settings
