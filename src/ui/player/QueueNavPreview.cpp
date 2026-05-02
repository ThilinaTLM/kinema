// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "ui/player/QueueNavPreview.h"

namespace kinema::ui::player {

QueueNavPreview::QueueNavPreview(QObject* parent)
    : QObject(parent)
{
}

void QueueNavPreview::setPreview(const QString& title,
    const QString& subtitle, const QStringList& chips)
{
    if (m_title != title) {
        m_title = title;
        Q_EMIT titleChanged();
    }
    if (m_subtitle != subtitle) {
        m_subtitle = subtitle;
        Q_EMIT subtitleChanged();
    }
    if (m_chips != chips) {
        m_chips = chips;
        Q_EMIT chipsChanged();
    }
    if (!m_available) {
        m_available = true;
        Q_EMIT availableChanged();
    }
}

void QueueNavPreview::clear()
{
    if (m_title.isEmpty() && m_subtitle.isEmpty() && m_chips.isEmpty()
        && !m_available) {
        return;
    }

    if (!m_title.isEmpty()) {
        m_title.clear();
        Q_EMIT titleChanged();
    }
    if (!m_subtitle.isEmpty()) {
        m_subtitle.clear();
        Q_EMIT subtitleChanged();
    }
    if (!m_chips.isEmpty()) {
        m_chips.clear();
        Q_EMIT chipsChanged();
    }
    if (m_available) {
        m_available = false;
        Q_EMIT availableChanged();
    }
}

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV
