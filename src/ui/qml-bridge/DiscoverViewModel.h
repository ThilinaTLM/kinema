// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Discover.h"
#include "api/Media.h"

#include <QList>
#include <QObject>
#include <QString>

#include <QCoro/QCoroTask>

#include <functional>

namespace kinema::api {
class TmdbClient;
}

namespace kinema::controllers {
class TokenController;
}

namespace kinema::ui::qml {

class DiscoverSectionModel;

/**
 * View-model behind `DiscoverPage.qml`. Owns one
 * `DiscoverSectionModel` per content rail (Trending, Popular Series,
 * Now Playing, On The Air, Top Rated Movies, Top Rated Series), each
 * fed by a TMDB endpoint.
 *
 * Wraps `controllers::TokenController` for the configured / auth-
 * failed page-level state and `api::TmdbClient` for the actual
 * fetches. Each section refreshes independently with its own epoch
 * counter so a slow response can't clobber a fresher one.
 *
 * Exposed to QML via `setContextProperty("discoverVm", ...)` from
 * `MainController`. NOT registered with `QML_ELEMENT` because the
 * type lives in `kinema_core` and the QML module belongs to
 * `kinema_qml_app` — see handover deviation §1.
 */
class DiscoverViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QList<QObject*> sections READ sectionsList NOTIFY sectionsChanged)
    Q_PROPERTY(bool tmdbConfigured READ tmdbConfigured NOTIFY tmdbConfiguredChanged)
    Q_PROPERTY(bool authFailed READ authFailed NOTIFY authFailedChanged)
    Q_PROPERTY(QString placeholderTitle READ placeholderTitle NOTIFY placeholderChanged)
    Q_PROPERTY(QString placeholderBody READ placeholderBody NOTIFY placeholderChanged)

public:
    DiscoverViewModel(api::TmdbClient* tmdb,
        controllers::TokenController* tokens,
        QObject* parent = nullptr);

    QList<QObject*> sectionsList() const;
    bool tmdbConfigured() const noexcept { return m_tmdbConfigured; }
    bool authFailed() const noexcept { return m_authFailed; }
    QString placeholderTitle() const;
    QString placeholderBody() const;

    /// Used by tests / `MainController` to feed the section list at
    /// construction without spinning a real TMDB client.
    using Fetcher = std::function<QCoro::Task<QList<api::DiscoverItem>>()>;
    struct SectionSpec {
        QString title;
        Fetcher fetch;
    };

public Q_SLOTS:
    /// Re-fetch every section. Stops early when TMDB is unconfigured;
    /// resets `authFailed` so a new token can re-arm the page.
    void refresh();

    /// QML hook for "Show all →". Phase 03 stubs this with a passive
    /// notification; phase 04 wires it to push the real Browse page.
    void browseSection(int sectionIndex);

    /// QML hook for clicking a poster card. Routes to `openMovieRequested`
    /// or `openSeriesRequested` based on the item's kind. Phase 03
    /// stubs the real navigation in `MainController` with a passive
    /// notification until the detail pages land in phase 05.
    void activateItem(int sectionIndex, int row);

Q_SIGNALS:
    void sectionsChanged();
    void tmdbConfiguredChanged();
    void authFailedChanged();
    void placeholderChanged();

    /// Routed by `MainController` to a Kirigami passive notification
    /// or, in phase 04+, a real navigation push.
    void browseRequested(api::MediaKind kind, const QString& sectionTitle);
    void openMovieRequested(int tmdbId, const QString& title);
    void openSeriesRequested(int tmdbId, const QString& title);

private:
    void buildSections();
    void setTmdbConfigured(bool on);
    void setAuthFailed(bool on);

    /// Per-section coroutine, guarded by `m_epochs[idx]`. Errors are
    /// translated via `core::describeError` and reported by setting
    /// the section's State::Error; 401/403 latches `authFailed` once
    /// per refresh so the page swaps to the placeholder.
    QCoro::Task<void> loadSection(int idx);

    api::TmdbClient* m_tmdb;
    controllers::TokenController* m_tokens;

    QList<DiscoverSectionModel*> m_sections;
    QList<Fetcher> m_fetchers;
    QList<quint64> m_epochs;

    bool m_tmdbConfigured = false;
    bool m_authFailed = false;
};

} // namespace kinema::ui::qml
