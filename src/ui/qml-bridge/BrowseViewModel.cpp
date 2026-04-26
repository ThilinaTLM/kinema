// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/BrowseViewModel.h"

#include "api/TmdbClient.h"
#include "config/BrowseSettings.h"
#include "core/HttpErrorPresenter.h"
#include "kinema_debug.h"
#include "ui/qml-bridge/DiscoverSectionModel.h"

#include <KLocalizedString>

#include <QPointer>
#include <QVariantMap>

#include <algorithm>

namespace kinema::ui::qml {

namespace {

QString windowLabel(core::DateWindow w)
{
    using core::DateWindow;
    switch (w) {
    case DateWindow::PastMonth:
        return i18nc("@label browse window chip", "Past month");
    case DateWindow::Past3Months:
        return i18nc("@label browse window chip", "Past 3 months");
    case DateWindow::ThisYear:
        return i18nc("@label browse window chip", "This year");
    case DateWindow::Past3Years:
        return i18nc("@label browse window chip", "Past 3 years");
    case DateWindow::Any:
        return i18nc("@label browse window chip", "Any date");
    }
    return {};
}

QString sortLabel(api::DiscoverSort s)
{
    switch (s) {
    case api::DiscoverSort::Popularity:
        return i18nc("@label browse sort chip", "Most popular");
    case api::DiscoverSort::ReleaseDate:
        return i18nc("@label browse sort chip", "Newest first");
    case api::DiscoverSort::Rating:
        return i18nc("@label browse sort chip", "Highest rated");
    case api::DiscoverSort::TitleAsc:
        return i18nc("@label browse sort chip", "Title (A–Z)");
    }
    return {};
}

QString ratingLabel(int pct)
{
    // pct is 0,60,70,75,80 — same scale BrowseSettings persists.
    return i18nc("@label browse rating chip",
        "\u2605 %1+", QString::number(pct / 10.0, 'f', 1));
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
    DateWindow w = DateWindow::ThisYear;
    switch (window) {
    case static_cast<int>(DateWindow::PastMonth):
        w = DateWindow::PastMonth; break;
    case static_cast<int>(DateWindow::Past3Months):
        w = DateWindow::Past3Months; break;
    case static_cast<int>(DateWindow::ThisYear):
        w = DateWindow::ThisYear; break;
    case static_cast<int>(DateWindow::Past3Years):
        w = DateWindow::Past3Years; break;
    case static_cast<int>(DateWindow::Any):
        w = DateWindow::Any; break;
    default:
        return;
    }
    if (m_dateWindow == w) {
        return;
    }
    m_dateWindow = w;
    m_settings.setDateWindow(w);
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
    api::DiscoverSort s = api::DiscoverSort::Popularity;
    switch (sort) {
    case static_cast<int>(api::DiscoverSort::Popularity):
        s = api::DiscoverSort::Popularity; break;
    case static_cast<int>(api::DiscoverSort::ReleaseDate):
        s = api::DiscoverSort::ReleaseDate; break;
    case static_cast<int>(api::DiscoverSort::Rating):
        s = api::DiscoverSort::Rating; break;
    case static_cast<int>(api::DiscoverSort::TitleAsc):
        s = api::DiscoverSort::TitleAsc; break;
    default:
        return;
    }
    if (m_sort == s) {
        return;
    }
    m_sort = s;
    m_settings.setSort(s);
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

QVariantList BrowseViewModel::activeChipsList() const
{
    // "kind" identifies which filter the chip removes when the user
    // clicks the ×; QML reads it via `model.kind` in the chip row's
    // delegate. The `payload` carries any per-chip detail the
    // `removeChip(index)` slot needs to undo the chip — for genre
    // chips, the genre id; otherwise unset.
    QVariantList out;

    auto addChip = [&out](const QString& kind, const QString& label,
                       const QVariant& payload = {}) {
        QVariantMap m;
        m.insert(QStringLiteral("kind"), kind);
        m.insert(QStringLiteral("label"), label);
        if (payload.isValid()) {
            m.insert(QStringLiteral("payload"), payload);
        }
        out.append(m);
    };

    // The kind is shown by the segmented control in `BrowseFilterBar`;
    // emitting a redundant chip with no remove affordance read as a
    // label rather than a token, so it's intentionally not surfaced
    // here. `removeChip` no longer handles the "kind" kind for the
    // same reason.
    if (m_dateWindow != core::DateWindow::ThisYear) {
        addChip(QStringLiteral("dateWindow"), windowLabel(m_dateWindow));
    }
    if (m_sort != api::DiscoverSort::Popularity) {
        addChip(QStringLiteral("sort"), sortLabel(m_sort));
    }
    if (m_minRatingPct > 0) {
        addChip(QStringLiteral("rating"), ratingLabel(m_minRatingPct));
    }
    if (!m_hideObscure) {
        addChip(QStringLiteral("hideObscure"),
            i18nc("@label browse chip", "Include obscure"));
    }
    for (int gid : m_genreIds) {
        const auto it = std::find_if(m_availableGenres.cbegin(),
            m_availableGenres.cend(),
            [gid](const api::TmdbGenre& g) { return g.id == gid; });
        const QString name = (it != m_availableGenres.cend())
            ? it->name
            : QString::number(gid);
        addChip(QStringLiteral("genre"), name, gid);
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
    m_dateWindow = core::DateWindow::ThisYear;
    m_minRatingPct = 0;
    m_sort = api::DiscoverSort::Popularity;
    m_hideObscure = true;
    m_availableGenres.clear();
    persistAll();
    Q_EMIT filtersChanged();
    Q_EMIT availableGenresChanged();
    refresh();
}

void BrowseViewModel::removeChip(int index)
{
    const auto chips = activeChipsList();
    if (index < 0 || index >= chips.size()) {
        return;
    }
    const auto chip = chips.at(index).toMap();
    const auto kind = chip.value(QStringLiteral("kind")).toString();

    if (kind == QLatin1String("dateWindow")) {
        setDateWindow(static_cast<int>(core::DateWindow::ThisYear));
    } else if (kind == QLatin1String("sort")) {
        setSort(static_cast<int>(api::DiscoverSort::Popularity));
    } else if (kind == QLatin1String("rating")) {
        setMinRatingPct(0);
    } else if (kind == QLatin1String("hideObscure")) {
        setHideObscure(true);
    } else if (kind == QLatin1String("genre")) {
        const int gid
            = chip.value(QStringLiteral("payload")).toInt();
        auto next = m_genreIds;
        next.removeAll(gid);
        setGenreIds(std::move(next));
    }
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
    const auto k = (kind == static_cast<int>(api::MediaKind::Series))
        ? api::MediaKind::Series
        : api::MediaKind::Movie;
    api::DiscoverSort s = api::DiscoverSort::Popularity;
    switch (sort) {
    case static_cast<int>(api::DiscoverSort::Popularity):
        s = api::DiscoverSort::Popularity; break;
    case static_cast<int>(api::DiscoverSort::ReleaseDate):
        s = api::DiscoverSort::ReleaseDate; break;
    case static_cast<int>(api::DiscoverSort::Rating):
        s = api::DiscoverSort::Rating; break;
    case static_cast<int>(api::DiscoverSort::TitleAsc):
        s = api::DiscoverSort::TitleAsc; break;
    }

    m_kind = k;
    m_sort = s;
    // Clear refinements so the preset matches the rail the user
    // clicked into. `dateWindow` keeps `ThisYear` (matches the
    // widget defaults); rating drops to "any"; genres clear.
    m_genreIds.clear();
    m_availableGenres.clear();
    m_dateWindow = core::DateWindow::ThisYear;
    m_minRatingPct = 0;
    m_hideObscure = true;
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
