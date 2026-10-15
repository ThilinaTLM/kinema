// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <KSharedConfig>

#include <QByteArray>
#include <QObject>

namespace kinema::config {

/**
 * Window / splitter geometry + close-to-tray.
 *
 * KConfig groups: [General], [MainWindow], and [PlayerWindow]
 * Keys:
 *   [General] closeToTray         bool
 *   [General] browseSplitter      QSplitter::saveState (opaque bytes)
 *   [General] seriesPaneSplitter  QSplitter::saveState (opaque bytes)
 *   [General] detailSplitter      QSplitter::saveState (opaque bytes)
 *   [MainWindow] ShowMenuBar      bool
 *   [PlayerWindow] Geometry       QWidget::saveGeometry() (opaque bytes)
 */
class AppearanceSettings : public QObject
{
    Q_OBJECT
public:
    explicit AppearanceSettings(KSharedConfigPtr config,
        QObject* parent = nullptr);

    bool closeToTray() const;
    void setCloseToTray(bool);

    QByteArray browseSplitterState() const;
    void setBrowseSplitterState(QByteArray);

    QByteArray seriesPaneSplitterState() const;
    void setSeriesPaneSplitterState(QByteArray);

    QByteArray detailSplitterState() const;
    void setDetailSplitterState(QByteArray);

    bool showMenuBar() const;
    void setShowMenuBar(bool);

    QByteArray playerWindowGeometry() const;
    void setPlayerWindowGeometry(QByteArray geometry);

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
