// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/BrowseFilterBar.h"

#include "config/BrowseSettings.h"

#include <KLocalizedString>

#include <QAction>
#include <QActionGroup>
#include <QCheckBox>
#include <QHBoxLayout>
#include <QIcon>
#include <QMenu>
#include <QToolButton>

namespace kinema::ui {

namespace {

// The rating buckets exposed in the UI. `0` means "Any" (omit the
// vote_average.gte filter); non-zero values are rating*10.
constexpr int kRatingBuckets[] = { 0, 60, 70, 75, 80 };

QString ratingLabel(int pct)
{
    if (pct <= 0) {
        return i18nc("@item:inmenu browse rating threshold", "Any rating");
    }
    // Render "7.5+" rather than "75%".
    const double r = pct / 10.0;
    return i18nc("@item:inmenu browse rating threshold (e.g. 7+)",
        "%1+", QString::number(r, 'g', 2));
}

QString windowLabel(core::DateWindow w)
{
    using core::DateWindow;
    switch (w) {
    case DateWindow::PastMonth:
        return i18nc("@item:inmenu browse date window", "Past month");
    case DateWindow::Past3Months:
        return i18nc("@item:inmenu browse date window", "Past 3 months");
    case DateWindow::ThisYear:
        return i18nc("@item:inmenu browse date window", "This year");
    case DateWindow::Past3Years:
        return i18nc("@item:inmenu browse date window", "Past 3 years");
    case DateWindow::Any:
        return i18nc("@item:inmenu browse date window", "Any time");
    }
    return {};
}

QString sortLabel(api::DiscoverSort s)
{
    using api::DiscoverSort;
    switch (s) {
    case DiscoverSort::Popularity:
        return i18nc("@item:inmenu browse sort axis", "Popularity");
    case DiscoverSort::ReleaseDate:
        return i18nc("@item:inmenu browse sort axis", "Newest");
    case DiscoverSort::Rating:
        return i18nc("@item:inmenu browse sort axis", "Highest rated");
    case DiscoverSort::TitleAsc:
        return i18nc("@item:inmenu browse sort axis", "Title A\u2013Z");
    }
    return {};
}

} // namespace

BrowseFilterBar::BrowseFilterBar(config::BrowseSettings& settings,
    QWidget* parent)
    : QWidget(parent)
    , m_settings(settings)
{
    // ---- layout skeleton -------------------------------------------------
    auto* layout = new QHBoxLayout(this);
    layout->setContentsMargins(8, 6, 8, 6);
    layout->setSpacing(8);

    m_kindBtn = new QToolButton(this);
    m_kindBtn->setPopupMode(QToolButton::InstantPopup);
    m_kindBtn->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    m_kindBtn->setAutoRaise(true);
    buildKindToggle();

    m_genresBtn = new QToolButton(this);
    m_genresBtn->setPopupMode(QToolButton::InstantPopup);
    m_genresBtn->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    m_genresBtn->setAutoRaise(true);
    m_genresBtn->setIcon(
        QIcon::fromTheme(QStringLiteral("view-filter")));
    // Populated lazily when setAvailableGenres() is called.
    m_genresMenu = new QMenu(m_genresBtn);
    m_genresBtn->setMenu(m_genresMenu);

    m_windowBtn = new QToolButton(this);
    m_windowBtn->setPopupMode(QToolButton::InstantPopup);
    m_windowBtn->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    m_windowBtn->setAutoRaise(true);
    m_windowBtn->setIcon(
        QIcon::fromTheme(QStringLiteral("view-calendar")));
    buildWindowMenu();

    m_ratingBtn = new QToolButton(this);
    m_ratingBtn->setPopupMode(QToolButton::InstantPopup);
    m_ratingBtn->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    m_ratingBtn->setAutoRaise(true);
    m_ratingBtn->setIcon(
        QIcon::fromTheme(QStringLiteral("rating")));
    buildRatingMenu();

    m_sortBtn = new QToolButton(this);
    m_sortBtn->setPopupMode(QToolButton::InstantPopup);
    m_sortBtn->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    m_sortBtn->setAutoRaise(true);
    m_sortBtn->setIcon(
        QIcon::fromTheme(QStringLiteral("view-sort")));
    buildSortMenu();

    m_hideObscure = new QCheckBox(
        i18nc("@option:check", "Hide obscure"), this);
    m_hideObscure->setToolTip(
        i18nc("@info:tooltip",
            "Require at least 200 votes on TMDB before showing a title."));
    connect(m_hideObscure, &QCheckBox::toggled, this,
        [this](bool on) { applyHideObscure(on); });

    layout->addWidget(m_kindBtn);
    layout->addWidget(m_genresBtn);
    layout->addWidget(m_windowBtn);
    layout->addWidget(m_ratingBtn);
    layout->addWidget(m_sortBtn);
    layout->addStretch(1);
    layout->addWidget(m_hideObscure);

    // ---- restore persisted state ----------------------------------------
    restoreFromConfig();
}

void BrowseFilterBar::buildKindToggle()
{
    auto* menu = new QMenu(m_kindBtn);
    auto* group = new QActionGroup(this);
    group->setExclusive(true);

    m_kindMovieAction = menu->addAction(
        i18nc("@action browse kind", "Movies"));
    m_kindMovieAction->setCheckable(true);
    m_kindMovieAction->setChecked(true);
    group->addAction(m_kindMovieAction);
    connect(m_kindMovieAction, &QAction::triggered, this,
        [this] { applyKind(api::MediaKind::Movie); });

    m_kindSeriesAction = menu->addAction(
        i18nc("@action browse kind", "TV Series"));
    m_kindSeriesAction->setCheckable(true);
    group->addAction(m_kindSeriesAction);
    connect(m_kindSeriesAction, &QAction::triggered, this,
        [this] { applyKind(api::MediaKind::Series); });

    m_kindBtn->setMenu(menu);
}

void BrowseFilterBar::buildWindowMenu()
{
    auto* menu = new QMenu(m_windowBtn);
    m_windowGroup = new QActionGroup(this);
    m_windowGroup->setExclusive(true);

    const core::DateWindow values[] = {
        core::DateWindow::PastMonth,
        core::DateWindow::Past3Months,
        core::DateWindow::ThisYear,
        core::DateWindow::Past3Years,
        core::DateWindow::Any,
    };
    for (auto w : values) {
        auto* a = menu->addAction(windowLabel(w));
        a->setCheckable(true);
        a->setData(static_cast<int>(w));
        m_windowGroup->addAction(a);
        connect(a, &QAction::triggered, this, [this, w] {
            applyWindow(w);
        });
    }
    m_windowBtn->setMenu(menu);
}

void BrowseFilterBar::buildRatingMenu()
{
    auto* menu = new QMenu(m_ratingBtn);
    m_ratingGroup = new QActionGroup(this);
    m_ratingGroup->setExclusive(true);

    for (int pct : kRatingBuckets) {
        auto* a = menu->addAction(ratingLabel(pct));
        a->setCheckable(true);
        a->setData(pct);
        m_ratingGroup->addAction(a);
        connect(a, &QAction::triggered, this, [this, pct] {
            applyRating(pct);
        });
    }
    m_ratingBtn->setMenu(menu);
}

void BrowseFilterBar::buildSortMenu()
{
    auto* menu = new QMenu(m_sortBtn);
    m_sortGroup = new QActionGroup(this);
    m_sortGroup->setExclusive(true);

    const api::DiscoverSort values[] = {
        api::DiscoverSort::Popularity,
        api::DiscoverSort::ReleaseDate,
        api::DiscoverSort::Rating,
        api::DiscoverSort::TitleAsc,
    };
    for (auto s : values) {
        auto* a = menu->addAction(sortLabel(s));
        a->setCheckable(true);
        a->setData(static_cast<int>(s));
        m_sortGroup->addAction(a);
        connect(a, &QAction::triggered, this, [this, s] {
            applySort(s);
        });
    }
    m_sortBtn->setMenu(menu);
}

QMenu* BrowseFilterBar::buildGenreMenu()
{
    auto* menu = new QMenu(m_genresBtn);

    // "Clear" convenience row when there's any selection at all.
    if (!m_genreIds.isEmpty()) {
        auto* clear = menu->addAction(
            QIcon::fromTheme(QStringLiteral("edit-clear")),
            i18nc("@action:inmenu", "Clear all"));
        connect(clear, &QAction::triggered, this, [this] {
            if (m_genreIds.isEmpty()) {
                return;
            }
            m_genreIds.clear();
            m_settings.setGenreIds({});
            m_genresMenu = buildGenreMenu();
            m_genresBtn->setMenu(m_genresMenu);
            applyGenreLabel();
            if (!m_muted) {
                Q_EMIT filtersChanged();
            }
        });
        menu->addSeparator();
    }

    for (const auto& g : m_availableGenres) {
        auto* a = menu->addAction(g.name);
        a->setCheckable(true);
        a->setChecked(m_genreIds.contains(g.id));
        a->setData(g.id);
        connect(a, &QAction::toggled, this, [this, id = g.id](bool on) {
            applyGenre(id, on);
        });
    }

    if (m_availableGenres.isEmpty()) {
        auto* placeholder = menu->addAction(
            i18nc("@item:inmenu placeholder while genre list is loading",
                "Loading\u2026"));
        placeholder->setEnabled(false);
    }

    return menu;
}

void BrowseFilterBar::applyKindLabel()
{
    if (m_kind == api::MediaKind::Movie) {
        m_kindBtn->setIcon(
            QIcon::fromTheme(QStringLiteral("video-x-generic")));
        m_kindBtn->setText(i18nc("@action browse kind", "Movies"));
    } else {
        m_kindBtn->setIcon(
            QIcon::fromTheme(QStringLiteral("view-media-recent")));
        m_kindBtn->setText(i18nc("@action browse kind", "TV Series"));
    }
}

void BrowseFilterBar::applyWindowLabel()
{
    m_windowBtn->setText(windowLabel(m_window));
    for (auto* a : m_windowGroup->actions()) {
        a->setChecked(a->data().toInt() == static_cast<int>(m_window));
    }
}

void BrowseFilterBar::applyRatingLabel()
{
    m_ratingBtn->setText(ratingLabel(m_minRatingPct));
    for (auto* a : m_ratingGroup->actions()) {
        a->setChecked(a->data().toInt() == m_minRatingPct);
    }
}

void BrowseFilterBar::applySortLabel()
{
    m_sortBtn->setText(sortLabel(m_sort));
    for (auto* a : m_sortGroup->actions()) {
        a->setChecked(a->data().toInt() == static_cast<int>(m_sort));
    }
}

void BrowseFilterBar::applyGenreLabel()
{
    if (m_genreIds.isEmpty()) {
        m_genresBtn->setText(
            i18nc("@action browse genre chooser, no selection", "Any genre"));
    } else {
        m_genresBtn->setText(
            i18ncp("@action browse genre chooser, N selected",
                "Genres (%1)", "Genres (%1)", m_genreIds.size()));
    }
}

void BrowseFilterBar::applyKind(api::MediaKind k)
{
    if (m_kind == k) {
        return;
    }
    m_kind = k;
    // When the kind changes the canonical genre ids differ entirely
    // (e.g. TMDB id 28 "Action" is movies-only; TV uses 10759 "Action
    // & Adventure"). Drop the persisted selection to avoid sending
    // cross-kind nonsense to /discover. The page will refetch the
    // genre list and feed it back in via setAvailableGenres().
    m_genreIds.clear();
    m_settings.setKind(k);
    m_settings.setGenreIds({});

    if (m_kindMovieAction && m_kindSeriesAction) {
        (k == api::MediaKind::Movie
                ? m_kindMovieAction
                : m_kindSeriesAction)->setChecked(true);
    }
    applyKindLabel();

    // Also rebuild the genre menu so the stale per-kind list isn't
    // shown until the page fetches the new one.
    m_availableGenres.clear();
    m_genresMenu = buildGenreMenu();
    m_genresBtn->setMenu(m_genresMenu);
    applyGenreLabel();

    if (!m_muted) {
        Q_EMIT filtersChanged();
    }
}

void BrowseFilterBar::applyWindow(core::DateWindow w)
{
    if (m_window == w) {
        return;
    }
    m_window = w;
    m_settings.setDateWindow(w);
    applyWindowLabel();
    if (!m_muted) {
        Q_EMIT filtersChanged();
    }
}

void BrowseFilterBar::applyRating(int ratingPct)
{
    if (m_minRatingPct == ratingPct) {
        return;
    }
    m_minRatingPct = ratingPct;
    m_settings.setMinRatingPct(ratingPct);
    applyRatingLabel();
    if (!m_muted) {
        Q_EMIT filtersChanged();
    }
}

void BrowseFilterBar::applySort(api::DiscoverSort s)
{
    if (m_sort == s) {
        return;
    }
    m_sort = s;
    m_settings.setSort(s);
    applySortLabel();
    if (!m_muted) {
        Q_EMIT filtersChanged();
    }
}

void BrowseFilterBar::applyGenre(int genreId, bool checked)
{
    const bool has = m_genreIds.contains(genreId);
    if (checked && !has) {
        m_genreIds.append(genreId);
    } else if (!checked && has) {
        m_genreIds.removeAll(genreId);
    } else {
        return;
    }
    m_settings.setGenreIds(m_genreIds);
    applyGenreLabel();
    if (!m_muted) {
        Q_EMIT filtersChanged();
    }
}

void BrowseFilterBar::applyHideObscure(bool on)
{
    if (m_hideObscureChecked == on) {
        return;
    }
    m_hideObscureChecked = on;
    m_settings.setHideObscure(on);
    if (!m_muted) {
        Q_EMIT filtersChanged();
    }
}

void BrowseFilterBar::setAvailableGenres(QList<api::TmdbGenre> genres)
{
    m_availableGenres = std::move(genres);

    // Drop persisted selections that no longer exist in the fresh
    // list. This handles two cases: (a) kind changed, applyKind()
    // already cleared m_genreIds; (b) TMDB removed a genre between
    // launches.
    QList<int> filtered;
    filtered.reserve(m_genreIds.size());
    for (int id : m_genreIds) {
        const bool stillThere = std::any_of(
            m_availableGenres.cbegin(), m_availableGenres.cend(),
            [id](const api::TmdbGenre& g) { return g.id == id; });
        if (stillThere) {
            filtered.append(id);
        }
    }
    const bool pruned = filtered.size() != m_genreIds.size();
    if (pruned) {
        m_genreIds = std::move(filtered);
        m_settings.setGenreIds(m_genreIds);
    }

    m_genresMenu = buildGenreMenu();
    m_genresBtn->setMenu(m_genresMenu);
    applyGenreLabel();

    if (pruned && !m_muted) {
        Q_EMIT filtersChanged();
    }
}

api::DiscoverQuery BrowseFilterBar::currentQuery() const
{
    api::DiscoverQuery q;
    q.kind = m_kind;
    q.withGenreIds = m_genreIds;
    const auto r = core::dateRangeFor(m_window);
    q.releasedGte = r.gte;
    q.releasedLte = r.lte;
    if (m_minRatingPct > 0) {
        q.voteAverageGte = m_minRatingPct / 10.0;
    }
    if (m_hideObscureChecked) {
        q.voteCountGte = 200;
    }
    q.sort = m_sort;
    q.page = 1;
    return q;
}

void BrowseFilterBar::resetToDefaults()
{
    m_muted = true;
    applyKind(api::MediaKind::Movie);
    applyWindow(core::DateWindow::ThisYear);
    applyRating(0);
    applySort(api::DiscoverSort::Popularity);
    if (!m_genreIds.isEmpty()) {
        m_genreIds.clear();
        m_settings.setGenreIds({});
        m_genresMenu = buildGenreMenu();
        m_genresBtn->setMenu(m_genresMenu);
        applyGenreLabel();
    }
    if (!m_hideObscureChecked) {
        m_hideObscureChecked = true;
        m_settings.setHideObscure(true);
        m_hideObscure->setChecked(true);
    }
    m_muted = false;
    Q_EMIT filtersChanged();
}

void BrowseFilterBar::restoreFromConfig()
{
    m_muted = true;

    m_kind = m_settings.kind();
    m_genreIds = m_settings.genreIds();
    m_window = m_settings.dateWindow();
    m_minRatingPct = m_settings.minRatingPct();
    m_sort = m_settings.sort();
    m_hideObscureChecked = m_settings.hideObscure();

    if (m_kindMovieAction && m_kindSeriesAction) {
        (m_kind == api::MediaKind::Movie
                ? m_kindMovieAction
                : m_kindSeriesAction)->setChecked(true);
    }
    applyKindLabel();
    applyWindowLabel();
    applyRatingLabel();
    applySortLabel();
    applyGenreLabel();

    m_hideObscure->setChecked(m_hideObscureChecked);

    m_muted = false;
}

} // namespace kinema::ui
