// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Media.h"
#include "domain/PlaybackContext.h"

#include <QAbstractListModel>
#include <QHash>
#include <QList>
#include <QString>

#include <optional>

namespace kinema::ui::qml {

struct LibraryListRow {
    domain::MediaKind kind = domain::MediaKind::Movie;
    QString imdbId;
    std::optional<int> season;
    std::optional<int> episode;
    QString title;
    QString subtitle;
    QString posterUrl;
    double progress = -1.0;
    bool watched = false;
    bool upcoming = false;
    QString releaseDateText;
    std::optional<domain::HistoryEntry> resumeEntry;
    std::optional<double> rating;
    std::optional<int> runtimeMinutes;
};

class LibraryListModel : public QAbstractListModel
{
    Q_OBJECT
    Q_PROPERTY(int count READ rowCount NOTIFY countChanged)
public:
    enum Roles {
        KindRole = Qt::UserRole + 1,
        ImdbIdRole,
        SeasonRole,
        EpisodeRole,
        TitleRole,
        SubtitleRole,
        PosterUrlRole,
        ProgressRole,
        WatchedRole,
        UpcomingRole,
        ReleaseDateTextRole,
        RatingRole,
        RuntimeMinutesRole,
    };
    Q_ENUM(Roles)

    explicit LibraryListModel(QObject* parent = nullptr);

    int rowCount(const QModelIndex& parent = {}) const override;
    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
    QHash<int, QByteArray> roleNames() const override;

    void setRows(QList<LibraryListRow> rows);
    const LibraryListRow* at(int row) const;
    const QList<LibraryListRow>& rows() const noexcept { return m_rows; }

Q_SIGNALS:
    void countChanged();

private:
    QList<LibraryListRow> m_rows;
};

} // namespace kinema::ui::qml
