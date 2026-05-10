// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/BrowseViewModel.h"

#include "api/TmdbClient.h"
#include "config/BrowseSettings.h"
#include "core/HttpErrorPresenter.h"
#include "ui/qml-bridge/DiscoverSectionModel.h"

#include <KLocalizedString>

#include <QPointer>
#include <QVariantMap>

#include <algorithm>
#include <initializer_list>
#include <optional>

namespace kinema::ui::qml {

namespace {

// Validate an int matches one of the listed enum values; returns the
// enum form when in-range, or `std::nullopt` so callers can early-out.
template <typename Enum>
std::optional<Enum> safeEnumCast(int value,
    std::initializer_list<Enum> valid)
{
    for (auto v : valid) {
        if (static_cast<int>(v) == value) {
            return v;
        }
    }
    return std::nullopt;
}

} // namespace

BrowseViewModel::BrowseViewModel(api::TmdbClient* tmdb,
    config::BrowseSettings& settings, QObject* parent)
    : QObject(parent)
    , m_tmdb(tmdb)
    , m_settings(settings)
    , m_results(new DiscoverSectionModel(
          i18nc("@title browse rail", "Browse"), this))
{
    restoreFromSettings();
    setTmdbConfigured(m_tmdb && m_tmdb->hasToken());
}

// ---- filter setters ---------------------------------------------------

void BrowseViewModel::setKind(int kind)
{
    const auto k = (kind == static_cast<int>(api::MediaKind::Series))
        ? api::MediaKind::Series
        : api::MediaKind::Movie;
    if (m_kind == k) {
        return;
    }
    m_kind = k;
    // Genre ids are kind-specific (TMDB id 28 "Action" is movies-only;
    // TV uses 10759 "Action & Adventure"). Drop the persisted set so
    // we don't pass cross-kind nonsense to /discover; the genre menu
    // refreshes lazily through `ensureGenresFor()`.
    m_genreIds.clear();
    m_availableGenres.clear();
    m_settings.setKind(m_kind);
    m_settings.setGenreIds({});
    Q_EMIT filtersChanged();
    Q_EMIT availableGenresChanged();
    refresh();
}

void BrowseViewModel::setGenreIds(QList<int> ids)
{
    if (m_genreIds == ids) {
        return;
    }
    m_genreIds = std::move(ids);
    m_settings.setGenreIds(m_genreIds);
    Q_EMIT filtersChanged();
    refresh();
}

void BrowseViewModel::setDateWindow(int window)
{
    using core::DateWindow;
    const auto w = safeEnumCast<DateWindow>(window, {
        DateWindow::PastMonth, DateWindow::Past3Months,
        DateWindow::ThisYear, DateWindow::Past3Years, DateWindow::Any });
    if (!w || m_dateWindow == *w) {
        return;
    }
    m_dateWindow = *w;
    m_settings.setDateWindow(*w);
    Q_EMIT filtersChanged();
    refresh();
}

void BrowseViewModel::setMinRatingPct(int pct)
{
    pct = std::clamp(pct, 0, 100);
    if (m_minRatingPct == pct) {
        return;
    }
    m_minRatingPct = pct;
    m_settings.setMinRatingPct(pct);
    Q_EMIT filtersChanged();
    refresh();
}

void BrowseViewModel::setSort(int sort)
{
    using api::DiscoverSort;
    const auto s = safeEnumCast<DiscoverSort>(sort, {
        DiscoverSort::Popularity, DiscoverSort::ReleaseDate,
        DiscoverSort::Rating, DiscoverSort::TitleAsc });
    if (!s || m_sort == *s) {
        return;
    }
    m_sort = *s;
    m_settings.setSort(*s);
    Q_EMIT filtersChanged();
    refresh();
}

void BrowseViewModel::setHideObscure(bool on)
{
    if (m_hideObscure == on) {
        return;
    }
    m_hideObscure = on;
    m_settings.setHideObscure(on);
    Q_EMIT filtersChanged();
    refresh();
}

// ---- accessors --------------------------------------------------------

QVariantList BrowseViewModel::availableGenresList() const
{
    QVariantList out;
    out.reserve(m_availableGenres.size());
    for (const auto& g : m_availableGenres) {
        QVariantMap m;
        m.insert(QStringLiteral("id"), g.id);
        m.insert(QStringLiteral("name"), g.name);
        m.insert(QStringLiteral("checked"), m_genreIds.contains(g.id));
        out.append(m);
    }
    return out;
}

bool BrowseViewModel::canLoadMore() const noexcept
{
    return m_tmdbConfigured && !m_authFailed && m_results->rowCount() > 0
        && m_totalPages > 0 && m_nextPage <= m_totalPages
        && !m_loading;
}

// ---- public slots -----------------------------------------------------

void BrowseViewModel::refresh()
{
    if (!m_tmdb || !m_tmdb->hasToken()) {
        setTmdbConfigured(false);
        m_results->setItems({});
        return;
    }
    setTmdbConfigured(true);
    setAuthFailed(false);

    // Fire the genre-list fetch in parallel; missing genres don't
    // block /discover.
    auto gt = ensureGenresFor(m_kind);
    Q_UNUSED(gt);

    auto t = loadPage(1, /*append=*/false);
    Q_UNUSED(t);
}

void BrowseViewModel::loadMore()
{
    if (!canLoadMore()) {
        return;
    }
    auto t = loadPage(m_nextPage, /*append=*/true);
    Q_UNUSED(t);
}

void BrowseViewModel::resetFilters()
{
    m_kind = api::MediaKind::Movie;
    m_genreIds.clear();
    m_dateWindow = core::DateWindow::Past3Years;
    m_minRatingPct = 0;
    m_sort = api::DiscoverSort::Popularity;
    m_hideObscure = false;
    m_availableGenres.clear();
    persistAll();
    Q_EMIT filtersChanged();
    Q_EMIT availableGenresChanged();
    refresh();
}

void BrowseViewModel::activate(int row)
{
    const auto* item = m_results->itemAt(row);
    if (!item) {
        return;
    }
    if (item->kind == api::MediaKind::Series) {
        Q_EMIT openSeriesRequested(item->tmdbId, item->title);
    } else {
        Q_EMIT openMovieRequested(item->tmdbId, item->title);
    }
}

void BrowseViewModel::applyPreset(int kind, int sort)
{
    using api::DiscoverSort;
    const auto k = (kind == static_cast<int>(api::MediaKind::Series))
        ? api::MediaKind::Series
        : api::MediaKind::Movie;
    const auto s = safeEnumCast<DiscoverSort>(sort, {
        DiscoverSort::Popularity, DiscoverSort::ReleaseDate,
        DiscoverSort::Rating, DiscoverSort::TitleAsc })
        .value_or(DiscoverSort::Popularity);

    m_kind = k;
    m_sort = s;
    // Clear refinements so the preset matches the rail the user
    // clicked into. DateWindow and obscure use the same defaults
    // as a fresh browse; rating drops to "any"; genres clear.
    m_genreIds.clear();
    m_availableGenres.clear();
    m_dateWindow = core::DateWindow::Past3Years;
    m_minRatingPct = 0;
    m_hideObscure = false;
    persistAll();

    Q_EMIT filtersChanged();
    Q_EMIT availableGenresChanged();
    refresh();
}

// ---- private ----------------------------------------------------------

void BrowseViewModel::restoreFromSettings()
{
    m_kind = m_settings.kind();
    m_genreIds = m_settings.genreIds();
    m_dateWindow = m_settings.dateWindow();
    m_minRatingPct = m_settings.minRatingPct();
    m_sort = m_settings.sort();
    m_hideObscure = m_settings.hideObscure();
}

void BrowseViewModel::persistAll()
{
    m_settings.setKind(m_kind);
    m_settings.setGenreIds(m_genreIds);
    m_settings.setDateWindow(m_dateWindow);
    m_settings.setMinRatingPct(m_minRatingPct);
    m_settings.setSort(m_sort);
    m_settings.setHideObscure(m_hideObscure);
}

api::DiscoverQuery BrowseViewModel::buildQuery(int page) const
{
    api::DiscoverQuery q;
    q.kind = m_kind;
    q.withGenreIds = m_genreIds;
    const auto r = core::dateRangeFor(m_dateWindow);
    q.releasedGte = r.gte;
    q.releasedLte = r.lte;
    if (m_minRatingPct > 0) {
        q.voteAverageGte = m_minRatingPct / 10.0;
    }
    if (m_hideObscure) {
        q.voteCountGte = 200;
    }
    q.sort = m_sort;
    q.page = page;
    return q;
}

void BrowseViewModel::setLoading(bool on)
{
    if (m_loading == on) {
        return;
    }
    m_loading = on;
    Q_EMIT loadingChanged();
    Q_EMIT canLoadMoreChanged();
}

void BrowseViewModel::setTmdbConfigured(bool on)
{
    if (m_tmdbConfigured == on) {
        return;
    }
    m_tmdbConfigured = on;
    Q_EMIT tmdbConfiguredChanged();
    Q_EMIT canLoadMoreChanged();
}

void BrowseViewModel::setAuthFailed(bool on)
{
    if (m_authFailed == on) {
        return;
    }
    m_authFailed = on;
    Q_EMIT authFailedChanged();
    Q_EMIT canLoadMoreChanged();
}

QCoro::Task<void> BrowseViewModel::loadPage(int page, bool append)
{
    if (!m_tmdb) {
        co_return;
    }
    // Fresh fetches bump the epoch so a stale page-1 response from
    // a prior filter setting can't clobber a newer one. Append
    // requests reuse the current epoch so they stay coupled to the
    // results that fed them.
    const quint64 myEpoch = append ? m_epoch : ++m_epoch;
    setLoading(true);

    if (!append) {
        m_nextPage = 1;
        m_totalPages = 0;
        m_results->setLoading();
    }

    const auto q = buildQuery(page);

    try {
        auto result = co_await m_tmdb->discover(q);
        if (myEpoch != m_epoch) {
            co_return;
        }
        m_totalPages = result.totalPages;
        m_nextPage = page + 1;

        if (append) {
            m_results->appendItems(std::move(result.items));
        } else {
            m_results->setItems(std::move(result.items));
        }
        setLoading(false);
        Q_EMIT canLoadMoreChanged();
    } catch (const std::exception& e) {
        if (myEpoch != m_epoch) {
            co_return;
        }
        setLoading(false);
        if (core::isHttpStatus(e, 401)
            || core::isHttpStatus(e, 403)) {
            setAuthFailed(true);
            m_results->setItems({});
            co_return;
        }
        const auto msg = core::describeError(e, "browse/discover");
        if (append) {
            // Page-1 stays visible on a load-more failure; surface
            // the error inline so the user sees why pagination
            // stopped progressing.
            Q_EMIT statusMessage(msg, 6000);
        } else {
            m_results->setError(msg);
        }
    }
}

QCoro::Task<void> BrowseViewModel::ensureGenresFor(api::MediaKind kind)
{
    bool& loaded = kind == api::MediaKind::Movie
        ? m_genresLoadedMovie
        : m_genresLoadedSeries;
    bool& loading = kind == api::MediaKind::Movie
        ? m_genresLoadingMovie
        : m_genresLoadingSeries;
    if (loading) {
        co_return;
    }
    if (loaded) {
        // Already cached at TmdbClient level; we still need to
        // re-fill m_availableGenres after a kind switch wiped it.
        if (!m_tmdb || !m_availableGenres.isEmpty()) {
            co_return;
        }
    }
    loading = true;
    QPointer<BrowseViewModel> self(this);
    try {
        auto list = co_await m_tmdb->genreList(kind);
        if (!self) {
            co_return;
        }
        loading = false;
        loaded = true;
        if (kind == m_kind) {
            m_availableGenres = std::move(list);
            // Drop any persisted genre id that no longer exists.
            QList<int> filtered;
            filtered.reserve(m_genreIds.size());
            for (int id : m_genreIds) {
                const bool stillThere = std::any_of(
                    m_availableGenres.cbegin(),
                    m_availableGenres.cend(),
                    [id](const api::TmdbGenre& g) {
                        return g.id == id;
                    });
                if (stillThere) {
                    filtered.append(id);
                }
            }
            const bool pruned = filtered.size() != m_genreIds.size();
            if (pruned) {
                m_genreIds = std::move(filtered);
                m_settings.setGenreIds(m_genreIds);
                Q_EMIT filtersChanged();
            }
            Q_EMIT availableGenresChanged();
        }
    } catch (const std::exception& e) {
        if (!self) {
            co_return;
        }
        loading = false;
        // Non-fatal: the filter drawer keeps its "Loading…" state
        // until the next refresh tries again. Log via describeError
        // so the failure shows up in qDebug output but doesn't
        // disrupt the user.
        core::describeError(e, "browse/genres");
    }
}

} // namespace kinema::ui::qml
