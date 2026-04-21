// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "ui/DiscoverPage.h"

#include "api/TmdbClient.h"
#include "core/HttpError.h"
#include "kinema_debug.h"
#include "ui/DiscoverCardDelegate.h"
#include "ui/DiscoverRowModel.h"
#include "ui/DiscoverSection.h"
#include "ui/ImageLoader.h"
#include "ui/StateWidget.h"

#include <KLocalizedString>

#include <QFrame>
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QScrollArea>
#include <QStackedWidget>
#include <QVBoxLayout>

namespace kinema::ui {

DiscoverPage::DiscoverPage(
    api::TmdbClient* tmdb, ImageLoader* images, QWidget* parent)
    : QWidget(parent)
    , m_tmdb(tmdb)
    , m_images(images)
{
    // Page-level call-to-action (shown when TMDB is not configured
    // or the token was rejected). Unlike StateWidget, this widget has
    // an explicit "Open Settings" button that MainWindow wires to
    // SettingsDialog.
    m_pageCta = new QWidget(this);
    {
        auto* icon = new QLabel(m_pageCta);
        icon->setAlignment(Qt::AlignCenter);
        icon->setPixmap(QIcon::fromTheme(QStringLiteral("applications-multimedia"))
                            .pixmap(QSize(64, 64)));

        m_pageCtaTitle = new QLabel(m_pageCta);
        m_pageCtaTitle->setAlignment(Qt::AlignCenter);
        auto f = m_pageCtaTitle->font();
        f.setPointSizeF(f.pointSizeF() * 1.2);
        f.setBold(true);
        m_pageCtaTitle->setFont(f);
        m_pageCtaTitle->setWordWrap(true);

        m_pageCtaBody = new QLabel(m_pageCta);
        m_pageCtaBody->setAlignment(Qt::AlignCenter);
        m_pageCtaBody->setWordWrap(true);
        m_pageCtaBody->setTextFormat(Qt::PlainText);

        auto* openSettings = new QPushButton(
            QIcon::fromTheme(QStringLiteral("configure")),
            i18nc("@action:button", "Open Settings"), m_pageCta);
        connect(openSettings, &QPushButton::clicked,
            this, &DiscoverPage::settingsRequested);

        auto* btnRow = new QHBoxLayout;
        btnRow->addStretch();
        btnRow->addWidget(openSettings);
        btnRow->addStretch();

        auto* v = new QVBoxLayout(m_pageCta);
        v->addStretch();
        v->addWidget(icon);
        v->addWidget(m_pageCtaTitle);
        v->addWidget(m_pageCtaBody);
        v->addLayout(btnRow);
        v->addStretch();
    }

    // Sections surface.
    m_rowsContainer = new QWidget;
    m_rowsLayout = new QVBoxLayout(m_rowsContainer);
    m_rowsLayout->setContentsMargins(16, 12, 16, 16);
    m_rowsLayout->setSpacing(16);

    m_rowsScroll = new QScrollArea(this);
    m_rowsScroll->setWidgetResizable(true);
    m_rowsScroll->setFrameShape(QFrame::NoFrame);
    m_rowsScroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    m_rowsScroll->setWidget(m_rowsContainer);

    m_pageStack = new QStackedWidget(this);
    m_pageStack->addWidget(m_rowsScroll); // idx 0 = sections
    m_pageStack->addWidget(m_pageCta);    // idx 1 = call-to-action

    auto* root = new QVBoxLayout(this);
    root->setContentsMargins(0, 0, 0, 0);
    root->addWidget(m_pageStack);

    buildSections();

    // Initial state: show sections with per-section loading
    // placeholders. The owning MainWindow is expected to call
    // refresh() or showNotConfigured() once it has resolved the
    // effective token.
    m_pageStack->setCurrentWidget(m_rowsScroll);
    for (auto& s : m_sections) {
        s.widget->showLoading();
    }
}

void DiscoverPage::buildSections()
{
    struct Spec {
        QString label;
        std::function<QCoro::Task<QList<api::DiscoverItem>>(api::TmdbClient*)> fetch;
    };

    const QVector<Spec> specs = {
        { i18nc("@label discover row", "Trending this week"),
            [](api::TmdbClient* c) {
                return c->trending(api::MediaKind::Movie, /*weekly=*/true);
            } },
        { i18nc("@label discover row", "Popular series"),
            [](api::TmdbClient* c) {
                return c->popular(api::MediaKind::Series);
            } },
        { i18nc("@label discover row", "Now playing in theaters"),
            [](api::TmdbClient* c) { return c->nowPlayingMovies(); } },
        { i18nc("@label discover row", "On the air"),
            [](api::TmdbClient* c) { return c->onTheAirSeries(); } },
        { i18nc("@label discover row", "Top rated movies"),
            [](api::TmdbClient* c) {
                return c->topRated(api::MediaKind::Movie);
            } },
        { i18nc("@label discover row", "Top rated series"),
            [](api::TmdbClient* c) {
                return c->topRated(api::MediaKind::Series);
            } },
    };

    m_sections.reserve(specs.size());

    for (int i = 0; i < specs.size(); ++i) {
        const auto& spec = specs.at(i);
        Section s;
        s.widget = new DiscoverSection(spec.label, m_images, m_rowsContainer);

        auto* tmdb = m_tmdb;
        const auto fetcher = spec.fetch;
        s.fetch = [tmdb, fetcher]() { return fetcher(tmdb); };

        const int sectionIndex = i;
        connect(s.widget, &DiscoverSection::itemActivated, this,
            [this, sectionIndex](const QModelIndex& idx) {
                onSectionActivated(sectionIndex, idx);
            });
        connect(s.widget, &DiscoverSection::retryRequested, this,
            [this, sectionIndex] {
                auto t = loadSection(sectionIndex);
                Q_UNUSED(t);
            });

        m_rowsLayout->addWidget(s.widget);
        m_sections.append(std::move(s));
    }

    m_rowsLayout->addStretch(1);
}

void DiscoverPage::onSectionActivated(int sectionIndex, const QModelIndex& idx)
{
    if (sectionIndex < 0 || sectionIndex >= m_sections.size() || !idx.isValid()) {
        return;
    }
    const auto* item = m_sections.at(sectionIndex).widget->model()->at(idx.row());
    if (!item) {
        return;
    }
    Q_EMIT itemActivated(*item);
}

void DiscoverPage::showNotConfigured()
{
    m_pageAuthFailed = false;
    m_pageCtaTitle->setText(
        i18nc("@label", "Set up TMDB to enable Discover"));
    m_pageCtaBody->setText(
        i18nc("@info",
            "Discover uses The Movie Database to surface trending, popular, "
            "and top-rated content. Open Settings → TMDB (Discover) and "
            "paste a v4 Read Access Token to get started."));
    m_pageStack->setCurrentWidget(m_pageCta);
}

void DiscoverPage::showPageError(const QString& title, const QString& body)
{
    m_pageCtaTitle->setText(title);
    m_pageCtaBody->setText(body);
    m_pageStack->setCurrentWidget(m_pageCta);
}

void DiscoverPage::refresh()
{
    m_pageAuthFailed = false;
    m_pageStack->setCurrentWidget(m_rowsScroll);

    for (auto& s : m_sections) {
        s.widget->delegate()->resetRequestTracking();
        s.widget->showLoading();
    }
    // Fire all sections concurrently.
    for (int i = 0; i < m_sections.size(); ++i) {
        auto t = loadSection(i);
        Q_UNUSED(t);
    }
}

QCoro::Task<void> DiscoverPage::loadSection(int index)
{
    if (index < 0 || index >= m_sections.size()) {
        co_return;
    }
    auto& s = m_sections[index];
    const auto myEpoch = ++s.epoch;

    s.widget->showLoading();

    try {
        auto items = co_await s.fetch();
        if (myEpoch != s.epoch) {
            co_return;
        }
        if (items.isEmpty()) {
            s.widget->model()->reset({});
            s.widget->showEmpty(
                i18nc("@info", "Nothing here right now."));
            co_return;
        }
        s.widget->model()->reset(std::move(items));
        s.widget->showItems();
    } catch (const core::HttpError& e) {
        if (myEpoch != s.epoch) {
            co_return;
        }
        qCWarning(KINEMA) << "Discover section" << index << "failed:"
                          << e.httpStatus() << e.message();
        if ((e.httpStatus() == 401 || e.httpStatus() == 403)
            && !m_pageAuthFailed) {
            m_pageAuthFailed = true;
            showPageError(
                i18nc("@label", "TMDB rejected the token"),
                i18nc("@info",
                    "The bundled TMDB token may have been revoked, or "
                    "your override is invalid. Open Settings → TMDB "
                    "(Discover) to paste a working token."));
            co_return;
        }
        if (m_pageAuthFailed) {
            // Page-level error already shown; don't clobber.
            co_return;
        }
        s.widget->showError(e.message(), /*retryable=*/true);
    } catch (const std::exception& e) {
        if (myEpoch != s.epoch) {
            co_return;
        }
        s.widget->showError(QString::fromUtf8(e.what()), /*retryable=*/true);
    }
}

} // namespace kinema::ui
