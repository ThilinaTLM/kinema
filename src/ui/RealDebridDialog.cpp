// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "ui/RealDebridDialog.h"

#include "api/RealDebridClient.h"
#include "config/Config.h"
#include "core/HttpClient.h"
#include "core/HttpError.h"
#include "core/TokenStore.h"
#include "kinema_debug.h"

#include <KLocalizedString>

#include <QDialogButtonBox>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QIcon>
#include <QLabel>
#include <QLineEdit>
#include <QLocale>
#include <QPushButton>
#include <QVBoxLayout>

namespace kinema::ui {

namespace {
constexpr auto kOkColor = "#2e7d32";
constexpr auto kErrorColor = "#c62828";
} // namespace

RealDebridDialog::RealDebridDialog(
    core::HttpClient* http, core::TokenStore* tokens, QWidget* parent)
    : QDialog(parent)
    , m_http(http)
    , m_tokens(tokens)
{
    setWindowTitle(i18nc("@title:window", "Real-Debrid"));
    setModal(true);
    resize(520, 260);

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

    m_removeButton = new QPushButton(
        QIcon::fromTheme(QStringLiteral("edit-delete")),
        i18nc("@action:button", "Remove token"), this);

    m_statusLabel = new QLabel(this);
    m_statusLabel->setWordWrap(true);
    m_statusLabel->setTextFormat(Qt::PlainText);
    m_statusLabel->setMinimumHeight(48);

    m_buttonBox = new QDialogButtonBox(
        QDialogButtonBox::Save | QDialogButtonBox::Cancel, this);

    auto* form = new QFormLayout;
    form->addRow(i18nc("@label:textbox", "API token:"), tokenRow);

    auto* actionRow = new QHBoxLayout;
    actionRow->addWidget(m_testButton);
    actionRow->addStretch(1);
    actionRow->addWidget(m_removeButton);

    auto* layout = new QVBoxLayout(this);
    layout->addWidget(intro);
    layout->addLayout(form);
    layout->addLayout(actionRow);
    layout->addWidget(m_statusLabel);
    layout->addStretch(1);
    layout->addWidget(m_buttonBox);

    connect(m_buttonBox, &QDialogButtonBox::accepted, this, [this] {
        auto t = saveToken();
        Q_UNUSED(t);
    });
    connect(m_buttonBox, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(m_testButton, &QPushButton::clicked, this, [this] {
        auto t = testConnection();
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

void RealDebridDialog::updateButtons()
{
    const bool hasText = !m_tokenEdit->text().trimmed().isEmpty();
    m_testButton->setEnabled(hasText);

    auto* saveBtn = m_buttonBox->button(QDialogButtonBox::Save);
    if (saveBtn) {
        saveBtn->setEnabled(hasText);
    }
    m_removeButton->setEnabled(config::Config::instance().hasRealDebrid());
}

void RealDebridDialog::setStatus(const QString& message, bool error)
{
    m_statusLabel->setText(message);
    m_statusLabel->setStyleSheet(
        QStringLiteral("color: %1; font-weight: 500;")
            .arg(QString::fromLatin1(error ? kErrorColor : kOkColor)));
}

void RealDebridDialog::clearStatus()
{
    m_statusLabel->clear();
    m_statusLabel->setStyleSheet(QString {});
}

QCoro::Task<void> RealDebridDialog::loadExistingToken()
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
        qCWarning(KINEMA) << "RD dialog: keyring read failed:" << e.what();
        setStatus(QString::fromUtf8(e.what()), /*error=*/true);
    }
    updateButtons();
}

QCoro::Task<void> RealDebridDialog::testConnection()
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
                    QLocale::system().toString(user.premiumUntil->date(),
                        QLocale::ShortFormat));
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

QCoro::Task<void> RealDebridDialog::saveToken()
{
    const auto token = m_tokenEdit->text().trimmed();
    if (token.isEmpty()) {
        co_return;
    }
    m_buttonBox->setEnabled(false);
    try {
        co_await m_tokens->write(
            QString::fromLatin1(core::TokenStore::kRealDebridKey), token);
        config::Config::instance().setHasRealDebrid(true);
        Q_EMIT tokenChanged(token);
        accept();
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), /*error=*/true);
        m_buttonBox->setEnabled(true);
    } catch (const std::exception& e) {
        setStatus(QString::fromUtf8(e.what()), /*error=*/true);
        m_buttonBox->setEnabled(true);
    }
}

QCoro::Task<void> RealDebridDialog::removeToken()
{
    m_removeButton->setEnabled(false);
    try {
        co_await m_tokens->remove(
            QString::fromLatin1(core::TokenStore::kRealDebridKey));
        config::Config::instance().setHasRealDebrid(false);
        Q_EMIT tokenChanged(QString {});
        accept();
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), /*error=*/true);
        m_removeButton->setEnabled(true);
    } catch (const std::exception& e) {
        setStatus(QString::fromUtf8(e.what()), /*error=*/true);
        m_removeButton->setEnabled(true);
    }
}

} // namespace kinema::ui
