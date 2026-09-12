// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>
#include <QString>

namespace kinema::ui::qml {

class AppIconResolver : public QObject
{
    Q_OBJECT

public:
    explicit AppIconResolver(QObject* parent = nullptr);

    Q_INVOKABLE QString url(const QString& iconId,
        const QString& hexColor) const;

private:
    QString ensureIconFile(const QString& iconId,
        const QString& hexColor) const;
};

} // namespace kinema::ui::qml
