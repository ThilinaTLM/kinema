// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "ui/qml-bridge/ListModelBase.h"

#include "domain/Media.h"
#include "domain/PlaybackContext.h"

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
    std::optional<int> year;
    std::optional<domain::HistoryEntry> resumeEntry;
    std::optional<double> rating;
    std::optional<int> runtimeMinutes;
};

class LibraryListModel : public ListModelBase<LibraryListRow>
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
        YearRole,
        RatingRole,
        RuntimeMinutesRole,
    };
    Q_ENUM(Roles)

    explicit LibraryListModel(QObject* parent = nullptr);

    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
    QHash<int, QByteArray> roleNames() const override;

    void setRows(QList<LibraryListRow> rows);

Q_SIGNALS:
    void countChanged();
};

} // namespace kinema::ui::qml
