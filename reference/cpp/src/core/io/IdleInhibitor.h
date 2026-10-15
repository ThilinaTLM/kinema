// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <memory>

#include <QObject>
#include <QString>

namespace kinema::core {

class IdleInhibitor : public QObject
{
    Q_OBJECT
public:
    class Backend {
    public:
        virtual ~Backend() = default;
        virtual QString name() const = 0;
        virtual bool inhibit(const QString& reason) = 0;
        virtual void uninhibit() = 0;
    };

    explicit IdleInhibitor(QObject* parent = nullptr);
    IdleInhibitor(std::unique_ptr<Backend> primary,
        std::unique_ptr<Backend> fallback,
        QObject* parent = nullptr);
    ~IdleInhibitor() override;

    void setActive(bool active, const QString& reason);
    bool isActive() const noexcept { return m_active; }

private:
    void release();

    std::unique_ptr<Backend> m_primary;
    std::unique_ptr<Backend> m_fallback;
    Backend* m_liveBackend = nullptr;
    bool m_active = false;
};

} // namespace kinema::core
