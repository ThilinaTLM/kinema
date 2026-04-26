// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Subtitle.h"

#include <QAbstractListModel>
#include <QHash>
#include <QSet>
#include <QString>

namespace kinema::ui::widgets {

/**
 * Single-column QAbstractListModel exposing the current
 * `controllers::SubtitleController` hits to `SubtitlesDialog`.
 *
 * The model is column-less by design — `SubtitleResultDelegate` paints
 * each row as a card using the role data, and `SubtitlesDialog` drives
 * the QTreeView in single-column mode. The cached / active flags are
 * stored as side sets here (rather than baked into `SubtitleHit`) so
 * the controller's hits list stays a pure mirror of the API response.
 */
class SubtitleResultsModel : public QAbstractListModel
{
    Q_OBJECT
public:
    enum Roles {
        FileIdRole = Qt::UserRole + 1,
        ReleaseRole,
        FileNameRole,
        LanguageRole,         ///< ISO 639-2 ("eng")
        LanguageNameRole,     ///< "English"
        FormatRole,           ///< "srt" / "vtt" / "ass"
        DownloadCountRole,
        RatingRole,
        UploaderRole,
        FpsRole,
        HearingImpairedRole,
        ForeignPartsOnlyRole,
        MoviehashMatchRole,
        CachedRole,           ///< already on disk
        ActiveRole,           ///< currently sideloaded into mpv
    };
    Q_ENUM(Roles)

    explicit SubtitleResultsModel(QObject* parent = nullptr);

    int rowCount(const QModelIndex& parent = {}) const override;
    QVariant data(const QModelIndex& index,
        int role = Qt::DisplayRole) const override;
    QHash<int, QByteArray> roleNames() const override;

    int count() const noexcept { return static_cast<int>(m_rows.size()); }

    /// Replace the row set. Resets the model.
    void setHits(const QList<api::SubtitleHit>& hits);

    /// Update the cached / active flags. Cheap; emits dataChanged
    /// across all rows for the affected role.
    void setCachedFileIds(const QSet<QString>& ids);
    void setActiveFileIds(const QSet<QString>& ids);

    const api::SubtitleHit& hitAt(int row) const { return m_rows.at(row); }

private:
    QList<api::SubtitleHit> m_rows;
    QSet<QString> m_cached;
    QSet<QString> m_active;
};

} // namespace kinema::ui::widgets
