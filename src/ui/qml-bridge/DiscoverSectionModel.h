// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Discover.h"

#include <QAbstractListModel>
#include <QHash>
#include <QList>
#include <QString>
#include <QStringList>
#include <QVariant>

namespace kinema::ui::qml {

/**
 * QML-friendly list model for one Discover rail (Trending, Popular,
 * Continue Watching, …).
 *
 * Replaces the widget-side `ui::DiscoverRowModel` for QML
 * consumers: same data, same parsing, but with named roles so QML
 * delegates can read `model.title`, `model.posterUrl`, etc. directly
 * — no `Qt::DisplayRole` plumbing.
 *
 * The model also carries per-rail metadata (`title`, `state`,
 * `errorMessage`) so a `ContentRail.qml` can render its own header
 * + placeholder without a separate adapter object.
 */
class DiscoverSectionModel : public QAbstractListModel
{
    Q_OBJECT
    Q_PROPERTY(QString title READ title WRITE setTitle NOTIFY titleChanged)
    Q_PROPERTY(State state READ state NOTIFY stateChanged)
    Q_PROPERTY(QString errorMessage READ errorMessage NOTIFY errorMessageChanged)
    Q_PROPERTY(int count READ rowCount NOTIFY countChanged)

public:
    enum Roles {
        TmdbIdRole = Qt::UserRole + 1,
        TitleRole,
        YearRole,
        PosterUrlRole,    ///< QString URL (QML wraps in image://kinema/poster?u=…)
        OverviewRole,
        VoteAverageRole,
        KindRole,         ///< api::MediaKind value
        ItemRole,         ///< QVariant<api::DiscoverItem> for activation handlers
        ProgressRole,     ///< double in [0,1]; -1 if not applicable
        LastReleaseRole,  ///< pre-formatted Continue-Watching subtitle line
    };
    Q_ENUM(Roles)

    /// Per-rail loading lifecycle. The QML rail watches `state` and
    /// shows a spinner / empty / error placeholder accordingly.
    enum class State {
        Idle = 0,
        Loading,
        Ready,
        Empty,
        Error,
    };
    Q_ENUM(State)

    explicit DiscoverSectionModel(QObject* parent = nullptr);
    explicit DiscoverSectionModel(QString title, QObject* parent = nullptr);

    // ---- Q_PROPERTY accessors --------------------------------------
    QString title() const { return m_title; }
    void setTitle(const QString& title);
    State state() const noexcept { return m_state; }
    QString errorMessage() const { return m_errorMessage; }

    // ---- QAbstractListModel ----------------------------------------
    int rowCount(const QModelIndex& parent = {}) const override;
    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
    QHash<int, QByteArray> roleNames() const override;

    // ---- Mutators (called by view-models) --------------------------
    void setLoading();
    void setItems(QList<api::DiscoverItem> items);
    void setError(const QString& message);

    /// Continue-Watching overlay data; ignored by other rails.
    void setProgressList(QList<double> progress);
    void setLastReleaseList(QStringList releases);

    /// Pure accessor for unit tests / hosting view-models.
    const QList<api::DiscoverItem>& items() const noexcept { return m_items; }
    const api::DiscoverItem* itemAt(int row) const;

Q_SIGNALS:
    void titleChanged();
    void stateChanged();
    void errorMessageChanged();
    void countChanged();

private:
    void resetState(State newState);

    QString m_title;
    QList<api::DiscoverItem> m_items;
    QList<double> m_progress;       ///< parallel to m_items; size matches or empty
    QStringList m_lastReleases;     ///< parallel to m_items; size matches or empty
    State m_state = State::Idle;
    QString m_errorMessage;
};

} // namespace kinema::ui::qml
