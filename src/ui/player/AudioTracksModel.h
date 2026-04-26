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
 * Read-only list model exposing the audio entries from mpv's
 * `track-list` to QML. Owned by `PlayerViewModel`; refreshed in
 * one shot via `reset()` whenever `MpvVideoItem::trackListChanged`
 * fires. The mpv track id is preserved on `IdRole` so the picker
 * can echo it back on selection.
 */
class AudioTracksModel : public QAbstractListModel
{
    Q_OBJECT
    QML_ELEMENT
    QML_UNCREATABLE("AudioTracksModel is owned by PlayerViewModel; "
                    "access via the audioTracks property.")
    /// QML cannot call the virtual `rowCount(parent)` directly, so
    /// the count is exposed as a property and re-emitted whenever the
    /// underlying list resets.
    Q_PROPERTY(int count READ count NOTIFY countChanged)
public:
    enum Roles {
        IdRole = Qt::UserRole + 1,
        TitleRole,         ///< human-readable label (lang + codec + ch)
        LangRole,
        CodecRole,
        ChannelsRole,
        SelectedRole,
    };
    Q_ENUM(Roles)

    explicit AudioTracksModel(QObject* parent = nullptr);

    int rowCount(const QModelIndex& parent = {}) const override;
    QVariant data(const QModelIndex& index,
        int role = Qt::DisplayRole) const override;
    QHash<int, QByteArray> roleNames() const override;

    /// Replace contents with the audio entries from `tracks`.
    /// Non-audio entries are filtered out. Emits a model reset.
    void reset(const core::tracks::TrackList& tracks);

    /// Currently selected mpv track id, or -1 when audio is off.
    /// Used by the picker to render the check mark and skip the
    /// re-selection call when the user re-picks the active row.
    int selectedId() const noexcept { return m_selectedId; }

    /// Convenience accessor for QML bindings; mirrors `rowCount({})`.
    int count() const noexcept { return static_cast<int>(m_rows.size()); }

Q_SIGNALS:
    void countChanged();

private:
    QList<core::tracks::Entry> m_rows;
    int m_selectedId = -1;
};

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV
