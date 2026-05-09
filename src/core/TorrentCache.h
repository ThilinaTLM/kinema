// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QDir>
#include <QHash>
#include <QObject>
#include <QSet>
#include <QString>

namespace kinema::config {
class TorrentStreamingSettings;
}

namespace kinema::core {

/**
 * Filesystem-backed cache for built-in torrent streaming. The cache
 * keeps one directory per info hash under `cache::torrentsDir()` and
 * prunes inactive directories least-recently-used by marker mtime.
 */
class TorrentCache : public QObject
{
    Q_OBJECT
public:
    explicit TorrentCache(const config::TorrentStreamingSettings& settings,
        QObject* parent = nullptr);

    QDir rootDir() const;
    QDir torrentDir(const QString& infoHash) const;
    QString resumeDataPath(const QString& infoHash) const;
    QString accessMarkerPath(const QString& infoHash) const;

    void markActive(const QString& infoHash);
    void markInactive(const QString& infoHash);
    bool isActive(const QString& infoHash) const;

    void touch(const QString& infoHash) const;
    qint64 sizeBytes() const;
    qint64 budgetBytes() const;

public Q_SLOTS:
    void enforceBudget();

private:
    static QString normalizedHash(QString infoHash);

    const config::TorrentStreamingSettings& m_settings;
    QSet<QString> m_active;
};

} // namespace kinema::core
