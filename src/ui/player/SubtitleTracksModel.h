// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include "core/MpvTrackList.h"

#include <QAbstractListModel>
#include <QHash>
#include <QtQmlIntegration/qqmlintegration.h>

namespace kinema::ui::player {

/**
 * Read-only list model exposing the subtitle entries from mpv's
 * `track-list` to QML. Mirrors `AudioTracksModel` in shape;
 * different roles surface forced / external flags that audio
 * doesn't have.
 *
 * The "Off" entry is **not** part of the model — the picker QML
 * adds a synthetic top row whose selected state is bound to
 * `selectedId === -1`. Keeping that policy in QML lets us
 * translate the label without dragging KLocalizedString into the
 * model.
 */
class SubtitleTracksModel : public QAbstractListModel
{
    Q_OBJECT
    QML_ELEMENT
    QML_UNCREATABLE("SubtitleTracksModel is owned by PlayerViewModel; "
                    "access via the subtitleTracks property.")
    Q_PROPERTY(int count READ count NOTIFY countChanged)
public:
    enum Roles {
        IdRole = Qt::UserRole + 1,
        TitleRole,
        LangRole,
        CodecRole,
        ForcedRole,
        ExternalRole,
        SelectedRole,
    };
    Q_ENUM(Roles)

    explicit SubtitleTracksModel(QObject* parent = nullptr);

    int rowCount(const QModelIndex& parent = {}) const override;
    QVariant data(const QModelIndex& index,
        int role = Qt::DisplayRole) const override;
    QHash<int, QByteArray> roleNames() const override;

    void reset(const core::tracks::TrackList& tracks);

    /// Currently selected mpv subtitle track id, or -1 when subs are
    /// disabled.
    int selectedId() const noexcept { return m_selectedId; }

    int count() const noexcept { return static_cast<int>(m_rows.size()); }

Q_SIGNALS:
    void countChanged();

private:
    QList<core::tracks::Entry> m_rows;
    int m_selectedId = -1;
};

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV
