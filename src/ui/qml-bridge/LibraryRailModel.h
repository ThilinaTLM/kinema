// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Media.h"

#include <QAbstractListModel>
#include <QDate>
#include <QHash>
#include <QList>
#include <QString>

#include <optional>

namespace kinema::ui::qml {

/// One card in a Library page rail. Same shape across rails;
/// callers (the view-model) populate the three text lines and the
/// artwork URLs per their own semantics:
///
///   * Up Next        — next unwatched aired episode per saved series
///   * Airing Soon    — next upcoming item (episode or movie) by date
///   * Recently Added — most-recently-saved titles
///
/// Three text lines give the card room for show title + episode
/// designator + meta ("Airs Fri, Apr 26", resume %, "Added 2 days
/// ago"). Empty lines are hidden by the QML, so cards with two-
/// line content (movies) and three-line content (episodes) coexist
/// without forcing uniform whitespace.
struct LibraryRailRow {
    domain::MediaKind kind = domain::MediaKind::Movie;
    QString imdbId;
    std::optional<int> season;
    std::optional<int> episode;
    /// Series or movie title (the parent saved-library row).
    QString title;
    /// 2:3 poster URL of the parent title. Always set when known;
    /// the QML uses it as a last-resort letterboxed fallback inside
    /// the rail's configured artwork frame when both `thumbnailUrl`
    /// and `backdropUrl` are unavailable.
    QString posterUrl;
    /// 16:9 backdrop URL of the parent title. Used as the preferred
    /// fallback for episode rails when an episode has no thumbnail
    /// (e.g. unaired Airing Soon entries) so the card renders the
    /// show's hero artwork at the same aspect as the frame instead
    /// of a letterboxed poster.
    QString backdropUrl;
    /// 16:9 episode still URL when the source actually had one.
    /// Empty for movies and for series rows whose Cinemeta payload
    /// didn't include a thumbnail — the card then walks
    /// `backdropUrl` → `posterUrl` → fallback icon.
    QString thumbnailUrl;
    QString primaryLine;
    QString secondaryLine;
    QString tertiaryLine;
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
        BackdropUrlRole,
        ThumbnailUrlRole,
        PrimaryLineRole,
        SecondaryLineRole,
        TertiaryLineRole,
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
