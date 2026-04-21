// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/Media.h"
#include "config/AppSettings.h"
#include "core/PlayerLauncher.h"
#include "services/StreamActions.h"
#include "ui/TorrentsModel.h"
#include "ui/widgets/StreamsPanel.h"

#include <KConfig>
#include <KSharedConfig>

#include <QApplication>
#include <QCheckBox>
#include <QTemporaryDir>
#include <QTest>

#include <memory>

using kinema::api::Stream;
using kinema::config::AppSettings;
using kinema::core::PlayerLauncher;
using kinema::services::StreamActions;
using kinema::ui::StreamsPanel;

class TstStreamsPanel : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void init()
    {
        m_tmpdir = std::make_unique<QTemporaryDir>();
        QVERIFY(m_tmpdir->isValid());
        m_config = KSharedConfig::openConfig(
            m_tmpdir->filePath(QStringLiteral("kinemarc")),
            KConfig::SimpleConfig);
        m_settings = std::make_unique<AppSettings>(m_config);
        m_launcher = std::make_unique<PlayerLauncher>(m_settings->player());
        m_actions = std::make_unique<StreamActions>(m_launcher.get());
    }

    void cleanup()
    {
        m_actions.reset();
        m_launcher.reset();
        m_settings.reset();
        m_config.reset();
        m_tmpdir.reset();
    }

    void testCachedOnlyAffectsVisibleRowCount()
    {
        StreamsPanel panel(m_settings->torrentio(), m_settings->filter(),
            *m_actions);
        panel.setRealDebridConfigured(true);

        Stream uncached;
        uncached.releaseName = QStringLiteral("Plain.Release.1080p");
        uncached.qualityLabel = QStringLiteral("1080p");
        uncached.resolution = QStringLiteral("1080p");
        uncached.seeders = 25;
        uncached.infoHash = QStringLiteral("1111111111111111111111111111111111111111");
        uncached.rdCached = false;

        Stream cached;
        cached.releaseName = QStringLiteral("Cached.Release.4K");
        cached.qualityLabel = QStringLiteral("2160p");
        cached.resolution = QStringLiteral("2160p");
        cached.seeders = 10;
        cached.infoHash = QStringLiteral("2222222222222222222222222222222222222222");
        cached.directUrl = QUrl(QStringLiteral("https://example.com/cached.mp4"));
        cached.rdCached = true;

        panel.setStreams({ uncached, cached });
        QCOMPARE(panel.model()->rowCount(), 2);

        auto* check = panel.findChild<QCheckBox*>();
        QVERIFY(check);
        QVERIFY(check->isVisible() || !panel.isVisible());

        m_settings->torrentio().setCachedOnly(true);
        QCOMPARE(panel.model()->rowCount(), 1);
        QCOMPARE(check->isChecked(), true);

        m_settings->torrentio().setCachedOnly(false);
        QCOMPARE(panel.model()->rowCount(), 2);
        QCOMPARE(check->isChecked(), false);

        check->setChecked(true);
        QCOMPARE(m_settings->torrentio().cachedOnly(), true);
        QCOMPARE(panel.model()->rowCount(), 1);
    }

    void testEmptyStreamListDoesNotCrash()
    {
        StreamsPanel panel(m_settings->torrentio(), m_settings->filter(),
            *m_actions);
        panel.setRealDebridConfigured(true);

        panel.setStreams({});
        QCOMPARE(panel.model()->rowCount(), 0);

        panel.showEmpty();
        QCOMPARE(panel.model()->rowCount(), 0);

        panel.showLoading();
        panel.showError(QStringLiteral("boom"));
        panel.showUnreleased(QDate(2030, 6, 1));
        QCOMPARE(panel.model()->rowCount(), 0);
    }

private:
    std::unique_ptr<QTemporaryDir> m_tmpdir;
    KSharedConfigPtr m_config;
    std::unique_ptr<AppSettings> m_settings;
    std::unique_ptr<PlayerLauncher> m_launcher;
    std::unique_ptr<StreamActions> m_actions;
};

QTEST_MAIN(TstStreamsPanel)
#include "tst_streams_panel.moc"
