// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>
#include <QString>
#include <QCoro/QCoroTask>

namespace kinema::api {
class IndexerSelector;
}

namespace kinema::domain {
enum class IndexerKind;
}

namespace kinema::ui::qml::settings {

/**
 * Shared status/busy state plus the connection-test flow for the
 * indexer settings sections (Torrentio / Peerflix). Each provider
 * supplies its `IndexerKind` and a display name; the base probes
 * that indexer via `IndexerSelector`.
 */
class IndexerSectionViewModelBase : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString statusMessage READ statusMessage NOTIFY statusChanged)
    Q_PROPERTY(int statusKind READ statusKind NOTIFY statusChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)

public:
    IndexerSectionViewModelBase(api::IndexerSelector* indexers,
        QObject* parent = nullptr);

    QString statusMessage() const { return m_statusMessage; }
    int statusKind() const { return m_statusKind; }
    bool busy() const { return m_busy; }

public Q_SLOTS:
    void testConnection();

Q_SIGNALS:
    void statusChanged();
    void busyChanged();

protected:
    virtual domain::IndexerKind indexerKind() const = 0;
    /// User-facing provider name for status messages.
    virtual QString providerName() const = 0;

    void setStatus(const QString& message, int kind);
    void setBusy(bool on);

    api::IndexerSelector* m_indexers;

private:
    QCoro::Task<void> testTask();

    QString m_statusMessage;
    int m_statusKind = 0;
    bool m_busy = false;
};

} // namespace kinema::ui::qml::settings
