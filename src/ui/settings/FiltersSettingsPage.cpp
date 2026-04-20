// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "ui/settings/FiltersSettingsPage.h"

#include "config/Config.h"

#include <KLocalizedString>

#include <QCheckBox>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QPlainTextEdit>
#include <QVBoxLayout>

namespace kinema::ui::settings {

namespace {

struct TokenOption {
    QString token; ///< Torrentio URL value
    QString label; ///< UI label (translated at call site)
};

QGroupBox* makeGroup(const QString& title,
    const QList<TokenOption>& options,
    QHash<QString, QCheckBox*>& out,
    QWidget* parent)
{
    auto* box = new QGroupBox(title, parent);
    auto* layout = new QVBoxLayout(box);
    for (const auto& opt : options) {
        auto* cb = new QCheckBox(opt.label, box);
        layout->addWidget(cb);
        out.insert(opt.token, cb);
    }
    layout->addStretch(1);
    return box;
}

} // namespace

FiltersSettingsPage::FiltersSettingsPage(QWidget* parent)
    : QWidget(parent)
{
    auto* intro = new QLabel(
        i18nc("@info",
            "Filters apply to every torrent search. Resolution and "
            "variant exclusions are enforced server-side by Torrentio; "
            "the keyword blocklist is applied locally."),
        this);
    intro->setWordWrap(true);

    // Resolution tokens — Torrentio groups 2160p and 1440p under "4k".
    const QList<TokenOption> resolutions {
        { QStringLiteral("4k"),
            i18nc("@option:check resolution", "4K (2160p & 1440p)") },
        { QStringLiteral("1080p"),
            i18nc("@option:check resolution", "1080p") },
        { QStringLiteral("720p"),
            i18nc("@option:check resolution", "720p") },
        { QStringLiteral("480p"),
            i18nc("@option:check resolution", "480p") },
        { QStringLiteral("other"),
            i18nc("@option:check resolution", "Other / unknown") },
    };

    const QList<TokenOption> categories {
        { QStringLiteral("cam"),
            i18nc("@option:check", "CAM") },
        { QStringLiteral("scr"),
            i18nc("@option:check", "Screener") },
        { QStringLiteral("threed"),
            i18nc("@option:check", "3D") },
        { QStringLiteral("hdr"),
            i18nc("@option:check", "HDR") },
        { QStringLiteral("hdr10plus"),
            i18nc("@option:check", "HDR10+") },
        { QStringLiteral("dolbyvision"),
            i18nc("@option:check", "Dolby Vision") },
        { QStringLiteral("nonen"),
            i18nc("@option:check", "Non-English") },
        { QStringLiteral("unknown"),
            i18nc("@option:check", "Unknown quality") },
        { QStringLiteral("brremux"),
            i18nc("@option:check", "BluRay REMUX") },
    };

    auto* resBox = makeGroup(
        i18nc("@title:group", "Exclude resolutions"),
        resolutions, m_resolutionChecks, this);
    auto* catBox = makeGroup(
        i18nc("@title:group", "Exclude variants"),
        categories, m_categoryChecks, this);

    auto* row = new QHBoxLayout;
    row->addWidget(resBox);
    row->addWidget(catBox, 1);

    m_blocklistEdit = new QPlainTextEdit(this);
    m_blocklistEdit->setPlaceholderText(
        i18nc("@info:placeholder",
            "e.g. HINDI\nYIFY\nNSFW\n(one keyword per line)"));
    m_blocklistEdit->setTabChangesFocus(true);

    auto* blocklistHint = new QLabel(
        i18nc("@info",
            "Case-insensitive substring match against the release name "
            "and its description line."),
        this);
    blocklistHint->setWordWrap(true);
    blocklistHint->setEnabled(false);

    auto* blocklistBox = new QGroupBox(
        i18nc("@title:group", "Keyword blocklist"), this);
    auto* blocklistLayout = new QVBoxLayout(blocklistBox);
    blocklistLayout->addWidget(m_blocklistEdit, 1);
    blocklistLayout->addWidget(blocklistHint);

    auto* layout = new QVBoxLayout(this);
    layout->addWidget(intro);
    layout->addLayout(row);
    layout->addWidget(blocklistBox, 1);

    load();
}

void FiltersSettingsPage::load()
{
    const auto& cfg = config::Config::instance();

    const auto resSet = cfg.excludedResolutions();
    for (auto it = m_resolutionChecks.constBegin();
         it != m_resolutionChecks.constEnd(); ++it) {
        it.value()->setChecked(resSet.contains(it.key()));
    }

    const auto catSet = cfg.excludedCategories();
    for (auto it = m_categoryChecks.constBegin();
         it != m_categoryChecks.constEnd(); ++it) {
        it.value()->setChecked(catSet.contains(it.key()));
    }

    m_blocklistEdit->setPlainText(cfg.keywordBlocklist().join(QLatin1Char('\n')));
}

void FiltersSettingsPage::apply()
{
    auto& cfg = config::Config::instance();

    // Preserve a deterministic order so the rendered URL is stable.
    static const QStringList resOrder {
        QStringLiteral("4k"), QStringLiteral("1080p"), QStringLiteral("720p"),
        QStringLiteral("480p"), QStringLiteral("other"),
    };
    static const QStringList catOrder {
        QStringLiteral("cam"), QStringLiteral("scr"), QStringLiteral("threed"),
        QStringLiteral("hdr"), QStringLiteral("hdr10plus"),
        QStringLiteral("dolbyvision"), QStringLiteral("nonen"),
        QStringLiteral("unknown"), QStringLiteral("brremux"),
    };

    QStringList res;
    for (const auto& tok : resOrder) {
        auto* cb = m_resolutionChecks.value(tok);
        if (cb && cb->isChecked()) {
            res.append(tok);
        }
    }
    QStringList cats;
    for (const auto& tok : catOrder) {
        auto* cb = m_categoryChecks.value(tok);
        if (cb && cb->isChecked()) {
            cats.append(tok);
        }
    }

    cfg.setExcludedResolutions(res);
    cfg.setExcludedCategories(cats);

    QStringList keywords = m_blocklistEdit->toPlainText().split(
        QLatin1Char('\n'), Qt::SkipEmptyParts);
    // Trim whitespace; Config::setKeywordBlocklist will drop empties.
    for (auto& k : keywords) {
        k = k.trimmed();
    }
    cfg.setKeywordBlocklist(keywords);
}

void FiltersSettingsPage::resetToDefaults()
{
    for (auto* cb : std::as_const(m_resolutionChecks)) {
        cb->setChecked(false);
    }
    for (auto* cb : std::as_const(m_categoryChecks)) {
        cb->setChecked(false);
    }
    m_blocklistEdit->clear();
}

} // namespace kinema::ui::settings
