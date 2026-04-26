// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"

#include <QAbstractListModel>
#include <QHash>
#include <QList>
#include <QString>
#include <QVariant>

namespace kinema::ui::qml {

/**
 * QML-friendly list model backing the Search page's results grid.
 *
 * Replaces the widget-side `ui::ResultsModel` with the same payload
 * (`QList<api::MetaSummary>`) and the same role set, but lives in
 * `qml-bridge/` so it can be exposed via `setContextProperty` from
 * `SearchViewModel`. Carries its own `state` / `errorMessage` so the
 * QML page can switch on `model.state` the same way `ContentRail.qml`
 * does for Discover rails.
 *
 * Search submits return MetaSummary rows from Cinemeta — distinct
 * from `DiscoverSectionModel` which carries `api::DiscoverItem`s
 * from TMDB. We don't generalise across the two: Cinemeta and TMDB
 * disagree on what an id even is (IMDB vs TMDB), and the Search
 * page never wants TMDB rows in its grid.
 */
class ResultsListModel : public QAbstractListModel
{
    Q_OBJECT
    Q_PROPERTY(State state READ state NOTIFY stateChanged)
    Q_PROPERTY(QString errorMessage READ errorMessage NOTIFY errorMessageChanged)
    Q_PROPERTY(int count READ rowCount NOTIFY countChanged)
public:
    enum Roles {
        ImdbIdRole = Qt::UserRole + 1,
        TitleRole,
        YearRole,
        PosterUrlRole,    ///< QString URL (QML wraps in image://kinema/poster?u=…)
        DescriptionRole,
        RatingRole,
        KindRole,         ///< api::MediaKind value
        SummaryRole,      ///< full MetaSummary as QVariant for activation
    };
    Q_ENUM(Roles)

    /// Lifecycle states a results grid renders. The page binds a
    /// StackLayout (or Loader) to these and shows a placeholder for
    /// every state except `Results`.
    enum class State {
        Idle = 0,   ///< no query submitted yet
        Loading,
        Results,
        Empty,
        Error,
    };
    Q_ENUM(State)

    explicit ResultsListModel(QObject* parent = nullptr);

    State state() const noexcept { return m_state; }
    QString errorMessage() const { return m_errorMessage; }

    int rowCount(const QModelIndex& parent = {}) const override;
    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
    QHash<int, QByteArray> roleNames() const override;

    /// Mutators driven by `SearchViewModel`.
    void setIdle();
    void setLoading();
    void setResults(QList<api::MetaSummary> rows);
    void setError(const QString& message);

    /// Pure accessors for the view-model and its tests.
    const QList<api::MetaSummary>& rows() const noexcept { return m_rows; }
    const api::MetaSummary* at(int row) const;

Q_SIGNALS:
    void stateChanged();
    void errorMessageChanged();
    void countChanged();

private:
    void resetState(State newState);

    QList<api::MetaSummary> m_rows;
    State m_state = State::Idle;
    QString m_errorMessage;
};

} // namespace kinema::ui::qml
