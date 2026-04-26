// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/DiscoverViewModel.h"

#include "api/TmdbClient.h"
#include "controllers/TokenController.h"
#include "core/HttpErrorPresenter.h"
#include "kinema_debug.h"
#include "ui/qml-bridge/DiscoverSectionModel.h"

#include <KLocalizedString>

#include <QPointer>

namespace kinema::ui::qml {

DiscoverViewModel::DiscoverViewModel(api::TmdbClient* tmdb,
    controllers::TokenController* tokens, QObject* parent)
    : QObject(parent)
    , m_tmdb(tmdb)
    , m_tokens(tokens)
{
    buildSections();

    // Initial configured state from the token resolution chain.
    setTmdbConfigured(m_tmdb && m_tmdb->hasToken());

    if (m_tokens) {
        connect(m_tokens,
            &controllers::TokenController::tmdbTokenChanged,
            this, [this](const QString& token) {
                const bool nowConfigured = !token.isEmpty();
                setTmdbConfigured(nowConfigured);
                // A token change implicitly clears any prior auth
                // rejection: the new token deserves a fresh attempt.
                setAuthFailed(false);
                if (nowConfigured) {
                    refresh();
                } else {
                    for (auto* s : std::as_const(m_sections)) {
                        s->setItems({});
                    }
                }
            });
    }
}

QList<QObject*> DiscoverViewModel::sectionsList() const
{
    QList<QObject*> out;
    out.reserve(m_sections.size());
    for (auto* s : m_sections) {
        out.append(s);
    }
    return out;
}

QString DiscoverViewModel::placeholderTitle() const
{
    if (m_authFailed) {
        return i18nc("@label discover placeholder",
            "TMDB rejected the token");
    }
    if (!m_tmdbConfigured) {
        return i18nc("@label discover placeholder",
            "Set up TMDB to enable Discover");
    }
    return {};
}

QString DiscoverViewModel::placeholderBody() const
{
    if (m_authFailed) {
        return i18nc("@info discover placeholder",
            "The bundled TMDB token may have been revoked, or your "
            "override is invalid. Open Settings → TMDB (Discover) to "
            "paste a working token.");
    }
    if (!m_tmdbConfigured) {
        return i18nc("@info discover placeholder",
            "Discover uses The Movie Database to surface trending, "
            "popular, and top-rated content. Open Settings → TMDB "
            "(Discover) and paste a v4 Read Access Token to get "
            "started.");
    }
    return {};
}

void DiscoverViewModel::buildSections()
{
    // Same list and order as the legacy widget DiscoverPage::buildSections.
    // QPointer guards the lambdas so a future cancellation during
    // teardown doesn't dereference a deleted client.
    QPointer<api::TmdbClient> tmdb(m_tmdb);

    const QList<SectionSpec> specs = {
        { i18nc("@label discover row", "Trending this week"),
            [tmdb]() -> QCoro::Task<QList<api::DiscoverItem>> {
                if (!tmdb) {
                    co_return QList<api::DiscoverItem> {};
                }
                co_return co_await tmdb->trending(
                    api::MediaKind::Movie, /*weekly=*/true);
            },
            api::MediaKind::Movie,
            api::DiscoverSort::Popularity },
        { i18nc("@label discover row", "Popular series"),
            [tmdb]() -> QCoro::Task<QList<api::DiscoverItem>> {
                if (!tmdb) {
                    co_return QList<api::DiscoverItem> {};
                }
                co_return co_await tmdb->popular(api::MediaKind::Series);
            },
            api::MediaKind::Series,
            api::DiscoverSort::Popularity },
        { i18nc("@label discover row", "Now playing in theaters"),
            [tmdb]() -> QCoro::Task<QList<api::DiscoverItem>> {
                if (!tmdb) {
                    co_return QList<api::DiscoverItem> {};
                }
                co_return co_await tmdb->nowPlayingMovies();
            },
            api::MediaKind::Movie,
            api::DiscoverSort::ReleaseDate },
        { i18nc("@label discover row", "On the air"),
            [tmdb]() -> QCoro::Task<QList<api::DiscoverItem>> {
                if (!tmdb) {
                    co_return QList<api::DiscoverItem> {};
                }
                co_return co_await tmdb->onTheAirSeries();
            },
            api::MediaKind::Series,
            api::DiscoverSort::ReleaseDate },
        { i18nc("@label discover row", "Top rated movies"),
            [tmdb]() -> QCoro::Task<QList<api::DiscoverItem>> {
                if (!tmdb) {
                    co_return QList<api::DiscoverItem> {};
                }
                co_return co_await tmdb->topRated(api::MediaKind::Movie);
            },
            api::MediaKind::Movie,
            api::DiscoverSort::Rating },
        { i18nc("@label discover row", "Top rated series"),
            [tmdb]() -> QCoro::Task<QList<api::DiscoverItem>> {
                if (!tmdb) {
                    co_return QList<api::DiscoverItem> {};
                }
                co_return co_await tmdb->topRated(api::MediaKind::Series);
            },
            api::MediaKind::Series,
            api::DiscoverSort::Rating },
    };

    m_sections.reserve(specs.size());
    m_fetchers.reserve(specs.size());
    m_browseKinds.reserve(specs.size());
    m_browseSorts.reserve(specs.size());
    m_epochs.reserve(specs.size());

    for (const auto& spec : specs) {
        auto* model = new DiscoverSectionModel(spec.title, this);
        m_sections.append(model);
        m_fetchers.append(spec.fetch);
        m_browseKinds.append(spec.browseKind);
        m_browseSorts.append(spec.browseSort);
        m_epochs.append(0);
    }

    Q_EMIT sectionsChanged();
}

void DiscoverViewModel::setTmdbConfigured(bool on)
{
    if (m_tmdbConfigured == on) {
        return;
    }
    m_tmdbConfigured = on;
    Q_EMIT tmdbConfiguredChanged();
    Q_EMIT placeholderChanged();
}

void DiscoverViewModel::setAuthFailed(bool on)
{
    if (m_authFailed == on) {
        return;
    }
    m_authFailed = on;
    Q_EMIT authFailedChanged();
    Q_EMIT placeholderChanged();
}

void DiscoverViewModel::refresh()
{
    setAuthFailed(false);

    if (!m_tmdbConfigured) {
        // Nothing to fetch — the placeholder takes the page surface.
        for (auto* s : std::as_const(m_sections)) {
            s->setItems({});
        }
        return;
    }

    for (int i = 0; i < m_sections.size(); ++i) {
        m_sections.at(i)->setLoading();
        auto t = loadSection(i);
        Q_UNUSED(t);
    }
}

QCoro::Task<void> DiscoverViewModel::loadSection(int idx)
{
    if (idx < 0 || idx >= m_sections.size()) {
        co_return;
    }
    // Capture-by-value of the index; epoch lookup re-reads m_epochs
    // each time so a parallel refresh of the same index supersedes
    // the in-flight call cleanly.
    const auto myEpoch = ++m_epochs[idx];
    auto* section = m_sections.at(idx);
    const auto& fetch = m_fetchers.at(idx);

    try {
        auto items = co_await fetch();
        if (myEpoch != m_epochs.at(idx)) {
            co_return;
        }
        section->setItems(std::move(items));
    } catch (const std::exception& e) {
        if (myEpoch != m_epochs.at(idx)) {
            co_return;
        }
        // 401 / 403 latches the page-level "auth failed" placeholder
        // once per refresh so we don't paint six identical errors
        // when every section trips on the same bad token.
        if ((core::isHttpStatus(e, 401) || core::isHttpStatus(e, 403))
            && !m_authFailed) {
            setAuthFailed(true);
            // Clear stale rail data behind the placeholder so a
            // recovered token can refill cleanly.
            for (auto* s : std::as_const(m_sections)) {
                s->setItems({});
            }
            co_return;
        }
        if (m_authFailed) {
            // Page-level placeholder already up; don't paint a
            // per-rail error on top.
            co_return;
        }
        const auto msg = core::describeError(e, "discover/section");
        section->setError(msg);
    }
}

void DiscoverViewModel::browseSection(int sectionIndex)
{
    if (sectionIndex < 0 || sectionIndex >= m_sections.size()) {
        return;
    }
    Q_EMIT browseRequested(
        static_cast<int>(m_browseKinds.at(sectionIndex)),
        static_cast<int>(m_browseSorts.at(sectionIndex)));
}

void DiscoverViewModel::activateItem(int sectionIndex, int row)
{
    if (sectionIndex < 0 || sectionIndex >= m_sections.size()) {
        return;
    }
    const auto* item = m_sections.at(sectionIndex)->itemAt(row);
    if (!item) {
        return;
    }
    if (item->kind == api::MediaKind::Series) {
        Q_EMIT openSeriesRequested(item->tmdbId, item->title);
    } else {
        Q_EMIT openMovieRequested(item->tmdbId, item->title);
    }
}

} // namespace kinema::ui::qml
