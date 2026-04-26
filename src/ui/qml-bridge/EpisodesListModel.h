// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"

#include <QAbstractListModel>
#include <QHash>
#include <QList>
#include <QString>

namespace kinema::ui::qml {

/**
 * QML-friendly list model for one season's episodes.
 *
 * Replaces the widget-side `ui::EpisodesModel` (`Qt::UserRole + N`
 * positional roles) with a flat named-roles list backing
 * `EpisodeRow.qml`. The owning view-model rebuilds the model whenever
 * the user flips seasons; there's no async state machine here \u2014
 * loading state lives at the page level on `metaState`.
 */
class EpisodesListModel : public QAbstractListModel
{
    Q_OBJECT
    Q_PROPERTY(int count READ rowCount NOTIFY countChanged)
public:
    enum Roles {
        NumberRole = Qt::UserRole + 1,
        TitleRole,
        DescriptionRole,
        ReleasedTextRole,    ///< pre-formatted release date string
        IsUpcomingRole,
        ThumbnailUrlRole,    ///< QString
        EpisodeRole,         ///< full api::Episode as QVariant
    };
    Q_ENUM(Roles)

    explicit EpisodesListModel(QObject* parent = nullptr);

    int rowCount(const QModelIndex& parent = {}) const override;
    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
    QHash<int, QByteArray> roleNames() const override;

    /// Replace the row list. Pure mutator \u2014 no implicit filtering.
    void setEpisodes(QList<api::Episode> rows);
    const QList<api::Episode>& episodes() const noexcept { return m_rows; }
    const api::Episode* at(int row) const;

    /// Find the row index for the given (season, number) pair, or -1.
    int rowFor(int season, int episodeNumber) const;

Q_SIGNALS:
    void countChanged();

private:
    QList<api::Episode> m_rows;
};

} // namespace kinema::ui::qml
