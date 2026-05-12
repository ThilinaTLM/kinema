// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>
#include <QString>
#include <QCoro/QCoroTask>

namespace kinema::api {
class IndexerSelector;
}

namespace kinema::config {
class PeerflixSettings;
}

namespace kinema::ui::qml::settings {

class PeerflixSectionViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString baseUrl READ baseUrl
        WRITE setBaseUrl NOTIFY baseUrlChanged)
    Q_PROPERTY(QString defaultBaseUrl READ defaultBaseUrlString CONSTANT)
    Q_PROPERTY(QString statusMessage READ statusMessage NOTIFY statusChanged)
    Q_PROPERTY(int statusKind READ statusKind NOTIFY statusChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)

public:
    PeerflixSectionViewModel(api::IndexerSelector* indexers,
        config::PeerflixSettings& settings,
        QObject* parent = nullptr);

    QString baseUrl() const;
    QString defaultBaseUrlString() const;
    QString statusMessage() const { return m_statusMessage; }
    int statusKind() const { return m_statusKind; }
    bool busy() const { return m_busy; }

    void setBaseUrl(const QString& url);

public Q_SLOTS:
    void testConnection();
    void resetBaseUrl();

Q_SIGNALS:
    void baseUrlChanged();
    void statusChanged();
    void busyChanged();

private:
    void setStatus(const QString& message, int kind);
    void setBusy(bool on);
    QCoro::Task<void> testTask();

    api::IndexerSelector* m_indexers;
    config::PeerflixSettings& m_settings;
    QString m_statusMessage;
    int m_statusKind = 0;
    bool m_busy = false;
};

} // namespace kinema::ui::qml::settings
