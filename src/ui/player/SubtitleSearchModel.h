// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include "api/Subtitle.h"

#include <QAbstractListModel>
#include <QHash>
#include <QSet>
#include <QString>
#include <QtQmlIntegration/qqmlintegration.h>

namespace kinema::ui::player {

/**
 * Read-only list model exposing the current `SubtitleController`
 * search hits to the player chrome.
 *
 * Mirrors `SubtitleTracksModel`'s shape: int role enum, raw fields
 * via roleNames(). Three transient flags annotate each row:
 *   - `cached`         — already on disk (file_id is in the cache),
 *   - `active`         — currently selected in mpv (sideloaded),
 *   - `moviehashMatch` — the upload's hash matched our movie hash.
 *
 * The QML picker sorts on the controller side; the model preserves
 * the order the controller hands it.
 */
class SubtitleSearchModel : public QAbstractListModel
{
    Q_OBJECT
    QML_ELEMENT
    QML_UNCREATABLE("SubtitleSearchModel is owned by PlayerViewModel; "
                    "access via the subtitleSearchModel property.")
    Q_PROPERTY(int count READ count NOTIFY countChanged)

public:
    enum Roles {
        FileIdRole = Qt::UserRole + 1,
        ReleaseRole,
        FileNameRole,
        LanguageRole,
        LanguageNameRole,
        DownloadCountRole,
        RatingRole,
        FormatRole,
        HearingImpairedRole,
        ForeignPartsOnlyRole,
        MoviehashMatchRole,
        CachedRole,
        ActiveRole,
        UploaderRole,
        FpsRole,
    };
    Q_ENUM(Roles)

    explicit SubtitleSearchModel(QObject* parent = nullptr);

    int rowCount(const QModelIndex& parent = {}) const override;
    QVariant data(const QModelIndex& index,
        int role = Qt::DisplayRole) const override;
    QHash<int, QByteArray> roleNames() const override;

    int count() const noexcept { return static_cast<int>(m_rows.size()); }

    void setHits(const QList<api::SubtitleHit>& hits);
    void setCachedFileIds(const QSet<QString>& ids);
    void setActiveFileIds(const QSet<QString>& ids);

Q_SIGNALS:
    void countChanged();

private:
    QList<api::SubtitleHit> m_rows;
    QSet<QString> m_cached;
    QSet<QString> m_active;
};

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV
