// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/IdleInhibitor.h"

#include <QDBusConnection>
#include <QDBusInterface>
#include <QDBusObjectPath>
#include <QDBusReply>
#include <QVariantMap>

#include <optional>

#include "kinema_log_app.h"

namespace kinema::core {

namespace {

constexpr uint kPortalInhibitIdle = 8;

class PortalInhibitBackend final : public IdleInhibitor::Backend
{
public:
    QString name() const override { return QStringLiteral("portal"); }

    bool inhibit(const QString& reason) override
    {
        QDBusInterface iface(QStringLiteral("org.freedesktop.portal.Desktop"),
            QStringLiteral("/org/freedesktop/portal/desktop"),
            QStringLiteral("org.freedesktop.portal.Inhibit"),
            QDBusConnection::sessionBus());
        if (!iface.isValid()) {
            return false;
        }
        QVariantMap options;
        options.insert(QStringLiteral("reason"), reason);
        const QDBusReply<QDBusObjectPath> reply = iface.call(
            QStringLiteral("Inhibit"), QString(), kPortalInhibitIdle,
            options);
        if (!reply.isValid() || reply.value().path().isEmpty()) {
            return false;
        }
        m_handle = reply.value().path();
        return true;
    }

    void uninhibit() override
    {
        if (m_handle.isEmpty()) {
            return;
        }
        QDBusInterface iface(QStringLiteral("org.freedesktop.portal.Desktop"),
            m_handle,
            QStringLiteral("org.freedesktop.portal.Request"),
            QDBusConnection::sessionBus());
        if (iface.isValid()) {
            iface.call(QStringLiteral("Close"));
        }
        m_handle.clear();
    }

private:
    QString m_handle;
};

class ScreenSaverBackend final : public IdleInhibitor::Backend
{
public:
    QString name() const override
    {
        return QStringLiteral("org.freedesktop.ScreenSaver");
    }

    bool inhibit(const QString& reason) override
    {
        QDBusInterface iface(QStringLiteral("org.freedesktop.ScreenSaver"),
            QStringLiteral("/ScreenSaver"),
            QStringLiteral("org.freedesktop.ScreenSaver"),
            QDBusConnection::sessionBus());
        if (!iface.isValid()) {
            return false;
        }
        const QDBusReply<uint> reply = iface.call(
            QStringLiteral("Inhibit"),
            QStringLiteral("dev.tlmtech.kinema"), reason);
        if (!reply.isValid()) {
            return false;
        }
        m_cookie = reply.value();
        return true;
    }

    void uninhibit() override
    {
        if (!m_cookie.has_value()) {
            return;
        }
        QDBusInterface iface(QStringLiteral("org.freedesktop.ScreenSaver"),
            QStringLiteral("/ScreenSaver"),
            QStringLiteral("org.freedesktop.ScreenSaver"),
            QDBusConnection::sessionBus());
        if (iface.isValid()) {
            iface.call(QStringLiteral("UnInhibit"), *m_cookie);
        }
        m_cookie.reset();
    }

private:
    std::optional<uint> m_cookie;
};

} // namespace

IdleInhibitor::IdleInhibitor(QObject* parent)
    : IdleInhibitor(std::make_unique<PortalInhibitBackend>(),
        std::make_unique<ScreenSaverBackend>(), parent)
{
}

IdleInhibitor::IdleInhibitor(std::unique_ptr<Backend> primary,
    std::unique_ptr<Backend> fallback,
    QObject* parent)
    : QObject(parent)
    , m_primary(std::move(primary))
    , m_fallback(std::move(fallback))
{
}

IdleInhibitor::~IdleInhibitor()
{
    release();
}

void IdleInhibitor::setActive(bool active, const QString& reason)
{
    if (active == m_active) {
        return;
    }
    if (!active) {
        release();
        return;
    }

    auto tryBackend = [&](Backend* backend) {
        if (!backend) {
            return false;
        }
        if (!backend->inhibit(reason)) {
            return false;
        }
        m_liveBackend = backend;
        m_active = true;
        qCInfo(KINEMA_APP) << "idle inhibitor enabled via" << backend->name();
        return true;
    };

    if (tryBackend(m_primary.get()) || tryBackend(m_fallback.get())) {
        return;
    }
    qCInfo(KINEMA_APP) << "idle inhibitor unavailable";
}

void IdleInhibitor::release()
{
    if (!m_active) {
        return;
    }
    if (m_liveBackend) {
        m_liveBackend->uninhibit();
    }
    m_liveBackend = nullptr;
    m_active = false;
}

} // namespace kinema::core
