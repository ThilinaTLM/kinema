// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/widgets/MetaHeaderWidget.h"

#include "core/DateFormat.h"
#include "ui/ImageLoader.h"
#include "ui/SimilarStrip.h"
#include "ui/StateWidget.h"
#include "ui/UpcomingBadge.h"

#include <KLocalizedString>

#include <QCoro/QCoroTask>
#include <QFrame>
#include <QHBoxLayout>
#include <QLabel>
#include <QPalette>
#include <QPixmap>
#include <QScrollArea>
#include <QStackedWidget>
#include <QVBoxLayout>

namespace kinema::ui {

MetaHeaderWidget::MetaHeaderWidget(ImageLoader* loader,
    api::TmdbClient* tmdb,
    QWidget* parent)
    : QWidget(parent)
    , m_loader(loader)
{
    if (tmdb) {
        m_similar = new SimilarStrip(tmdb, loader, this);
        connect(m_similar, &SimilarStrip::itemActivated,
            this, &MetaHeaderWidget::similarActivated);
    }

    m_state = new StateWidget(this);

    m_posterLabel = new QLabel(this);
    m_posterLabel->setFixedSize(220, 330);
    m_posterLabel->setAlignment(Qt::AlignCenter);
    m_posterLabel->setFrameShape(QFrame::StyledPanel);

    m_titleLabel = new QLabel(this);
    {
        auto tf = m_titleLabel->font();
        tf.setPointSizeF(tf.pointSizeF() * 1.4);
        tf.setBold(true);
        m_titleLabel->setFont(tf);
    }
    m_titleLabel->setWordWrap(true);

    m_upcomingBadge = makeUpcomingBadge(this);

    // Title row: title stretches so the badge sits to the right of
    // the visible title text regardless of wrap width.
    auto* titleRow = new QHBoxLayout;
    titleRow->setSpacing(8);
    titleRow->setContentsMargins(0, 0, 0, 0);
    titleRow->addWidget(m_titleLabel, 1);
    titleRow->addWidget(m_upcomingBadge, 0, Qt::AlignTop);

    m_metaLineLabel = new QLabel(this);
    m_metaLineLabel->setWordWrap(true);
    m_metaLineLabel->setTextFormat(Qt::PlainText);

    m_descLabel = new QLabel(this);
    m_descLabel->setWordWrap(true);
    m_descLabel->setAlignment(Qt::AlignTop | Qt::AlignLeft);
    m_descLabel->setTextFormat(Qt::PlainText);

    auto* metaRight = new QVBoxLayout;
    metaRight->setSpacing(6);
    metaRight->addLayout(titleRow);
    metaRight->addWidget(m_metaLineLabel);
    metaRight->addWidget(m_descLabel, 1);

    auto* metaRow = new QHBoxLayout;
    metaRow->setSpacing(12);
    metaRow->addWidget(m_posterLabel, 0, Qt::AlignTop);
    metaRow->addLayout(metaRight, 1);

    auto* content = new QWidget(this);
    auto* contentLayout = new QVBoxLayout(content);
    contentLayout->setContentsMargins(12, 12, 12, 12);
    contentLayout->setSpacing(8);
    contentLayout->addLayout(metaRow);
    if (m_similar) {
        contentLayout->addWidget(m_similar, 0);
    }
    contentLayout->addStretch(1);

    m_scroll = new QScrollArea(this);
    m_scroll->setFrameShape(QFrame::NoFrame);
    m_scroll->setWidgetResizable(true);
    m_scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    m_scroll->setWidget(content);
    // Blend the scroll viewport with the surrounding window chrome
    // instead of the darker QPalette::Base default.
    m_scroll->viewport()->setBackgroundRole(QPalette::Window);

    m_stack = new QStackedWidget(this);
    m_stack->addWidget(m_state);    // idx 0 = state
    m_stack->addWidget(m_scroll);   // idx 1 = content

    auto* root = new QVBoxLayout(this);
    root->setContentsMargins(0, 0, 0, 0);
    root->setSpacing(0);
    root->addWidget(m_stack, 1);

    if (m_loader) {
        connect(m_loader, &ImageLoader::posterReady, this,
            [this](const QUrl& url) {
                if (url == m_pendingPosterUrl) {
                    updatePoster();
                }
            });
    }

    showIdle();
}

void MetaHeaderWidget::showIdle()
{
    m_state->showIdle(
        i18nc("@label", "Nothing selected"),
        i18nc("@info", "Pick a result to see details and torrents."));
    m_stack->setCurrentWidget(m_state);

    m_titleLabel->clear();
    m_metaLineLabel->clear();
    m_descLabel->clear();
    m_posterLabel->clear();
    m_pendingPosterUrl.clear();
    if (m_upcomingBadge) {
        m_upcomingBadge->hide();
    }
    if (m_similar) {
        m_similar->setContextImdb(api::MediaKind::Movie, QString {});
    }
}

void MetaHeaderWidget::showLoading()
{
    m_state->showLoading(i18nc("@info:status", "Loading details…"));
    m_stack->setCurrentWidget(m_state);

    m_titleLabel->clear();
    m_metaLineLabel->clear();
    m_descLabel->clear();
    m_posterLabel->clear();
    m_pendingPosterUrl.clear();
    if (m_upcomingBadge) {
        m_upcomingBadge->hide();
    }
}

void MetaHeaderWidget::showError(const QString& message)
{
    m_state->showError(message, /*retryable=*/false);
    m_stack->setCurrentWidget(m_state);
}

void MetaHeaderWidget::setMeta(const api::MetaDetail& meta)
{
    m_titleLabel->setText(meta.summary.year
            ? QStringLiteral("%1 (%2)").arg(meta.summary.title).arg(*meta.summary.year)
            : meta.summary.title);

    QStringList metaLine;
    if (!meta.genres.isEmpty()) {
        metaLine << meta.genres.join(QStringLiteral(", "));
    }
    if (meta.runtimeMinutes) {
        metaLine << i18nc("@label runtime", "%1 min", *meta.runtimeMinutes);
    }
    if (meta.summary.imdbRating) {
        metaLine << i18nc("@label rating", "★ %1",
            QString::number(*meta.summary.imdbRating, 'f', 1));
    }
    m_metaLineLabel->setText(metaLine.join(QStringLiteral("  ·  ")));

    m_descLabel->setText(meta.summary.description);

    if (core::isFutureRelease(meta.summary.released)) {
        setUpcomingBadgeDate(m_upcomingBadge, *meta.summary.released);
        m_upcomingBadge->show();
    } else {
        m_upcomingBadge->hide();
    }

    m_pendingPosterUrl = meta.summary.poster;
    m_posterLabel->clear();
    updatePoster();

    m_stack->setCurrentWidget(m_scroll);
}

void MetaHeaderWidget::setSimilarContext(api::MediaKind kind, const QString& imdbId)
{
    if (m_similar) {
        m_similar->setContextImdb(kind, imdbId);
    }
}

void MetaHeaderWidget::updatePoster()
{
    if (!m_loader || m_pendingPosterUrl.isEmpty()) {
        return;
    }
    // Kick off a (possibly cache-hitting) fetch. The posterReady
    // signal wired in the ctor calls back into updatePoster() once
    // the fetch lands.
    auto task = m_loader->requestPoster(m_pendingPosterUrl);
    Q_UNUSED(task);
    const QPixmap pm = m_loader->cached(m_pendingPosterUrl);
    if (!pm.isNull()) {
        m_posterLabel->setPixmap(pm.scaled(m_posterLabel->size(),
            Qt::KeepAspectRatio, Qt::SmoothTransformation));
    }
}

} // namespace kinema::ui
