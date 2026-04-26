// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include "core/MpvChapterList.h"

#include <QAbstractListModel>
#include <QHash>
#include <QtQmlIntegration/qqmlintegration.h>

namespace kinema::ui::player {

/**
 * Read-only list model exposing mpv's `chapter-list` to QML. The
 * seek bar uses this to draw chapter ticks; tooltips read the
 * title role at the hovered tick.
 */
class ChaptersModel : public QAbstractListModel
{
    Q_OBJECT
    QML_ELEMENT
    QML_UNCREATABLE("ChaptersModel is owned by PlayerViewModel; "
                    "access via the chapters property.")
    Q_PROPERTY(int count READ count NOTIFY countChanged)
public:
    enum Roles {
        TimeRole = Qt::UserRole + 1, ///< start time, seconds (double)
        TitleRole,
    };
    Q_ENUM(Roles)

    explicit ChaptersModel(QObject* parent = nullptr);

    int rowCount(const QModelIndex& parent = {}) const override;
    QVariant data(const QModelIndex& index,
        int role = Qt::DisplayRole) const override;
    QHash<int, QByteArray> roleNames() const override;

    void reset(const core::chapters::ChapterList& chapters);

    int count() const noexcept { return static_cast<int>(m_rows.size()); }

Q_SIGNALS:
    void countChanged();

private:
    core::chapters::ChapterList m_rows;
};

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV
