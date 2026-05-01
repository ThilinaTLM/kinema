// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"

#include <QAbstractListModel>
#include <QDate>
#include <QHash>
#include <QList>
#include <QString>

#include <optional>

namespace kinema::ui::qml {

/// One card in a Library page rail. Same shape for all three rails
/// (Up Next, Airing Soon, Coming Up); the populating view-model
/// fills `primaryLine` / `secondaryLine` per rail semantics:
///
///   * Up Next       \u2014 next unwatched aired episode of a saved series
///   * Airing Soon   \u2014 next upcoming episode airing within ~30 days
///   * Coming Up     \u2014 saved movie with a future release date
struct LibraryRailRow {
    api::MediaKind kind = api::MediaKind::Movie;
    QString imdbId;
    std::optional<int> season;
    std::optional<int> episode;
    /// Series or movie title (the parent saved-library row).
    QString title;
    /// 2:3 poster URL of the parent title (used as fallback when
    /// `thumbnailUrl` is empty).
    QString posterUrl;
    /// 16:9 episode still URL when `kind == Series` and the rail is
    /// Up Next / Airing Soon. Empty for movies and as a fallback.
    QString thumbnailUrl;
    /// Bold first line on the card (e.g. "S02E08 \u2014 The Bell" or movie title).
    QString primaryLine;
    /// Caption second line on the card (e.g. "Airs Fri, Apr 26").
    QString secondaryLine;
    std::optional<QDate> releaseDate;
    /// Resume position fraction (0.0..1.0) when this row has playback
    /// progress, otherwise -1. Currently only Up Next surfaces this.
    double progress = -1.0;
};

class LibraryRailModel : public QAbstractListModel
{
    Q_OBJECT
    Q_PROPERTY(int count READ rowCount NOTIFY countChanged)
    Q_PROPERTY(bool empty READ empty NOTIFY countChanged)
public:
    enum Roles {
        KindRole = Qt::UserRole + 1,
        ImdbIdRole,
        SeasonRole,
        EpisodeRole,
        TitleRole,
        PosterUrlRole,
        ThumbnailUrlRole,
        PrimaryLineRole,
        SecondaryLineRole,
        ProgressRole,
    };
    Q_ENUM(Roles)

    explicit LibraryRailModel(QObject* parent = nullptr);

    int rowCount(const QModelIndex& parent = {}) const override;
    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
    QHash<int, QByteArray> roleNames() const override;

    bool empty() const noexcept { return m_rows.isEmpty(); }

    void setRows(QList<LibraryRailRow> rows);
    const LibraryRailRow* at(int row) const;
    const QList<LibraryRailRow>& rows() const noexcept { return m_rows; }

Q_SIGNALS:
    void countChanged();

private:
    QList<LibraryRailRow> m_rows;
};

} // namespace kinema::ui::qml
