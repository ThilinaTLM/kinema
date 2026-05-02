// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include <QObject>
#include <QString>
#include <QStringList>

namespace kinema::ui::player {

class QueueNavPreview : public QObject
{
    Q_OBJECT
    Q_PROPERTY(bool available READ available NOTIFY availableChanged)
    Q_PROPERTY(QString title READ title NOTIFY titleChanged)
    Q_PROPERTY(QString subtitle READ subtitle NOTIFY subtitleChanged)
    Q_PROPERTY(QStringList chips READ chips NOTIFY chipsChanged)

public:
    explicit QueueNavPreview(QObject* parent = nullptr);

    bool available() const noexcept { return m_available; }
    QString title() const { return m_title; }
    QString subtitle() const { return m_subtitle; }
    QStringList chips() const { return m_chips; }

    void setPreview(const QString& title, const QString& subtitle,
        const QStringList& chips);
    void clear();

Q_SIGNALS:
    void availableChanged();
    void titleChanged();
    void subtitleChanged();
    void chipsChanged();

private:
    bool m_available = false;
    QString m_title;
    QString m_subtitle;
    QStringList m_chips;
};

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV
