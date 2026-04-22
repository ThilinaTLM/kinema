// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/Media.h"
#include "config/AppSettings.h"
#include "core/Magnet.h"
#include "core/PlayerLauncher.h"
#include "services/StreamActions.h"

#include <KConfig>
#include <KSharedConfig>

#include <QApplication>
#include <QClipboard>
#include <QGuiApplication>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>
#include <QUrl>

using kinema::api::Stream;
using kinema::core::PlayerLauncher;
using kinema::services::StreamActions;

class TstStreamActions : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void init()
    {
        // Fresh KConfig per test so tray / close-to-tray defaults
        // don't leak into anything. We only need settings because
        // PlayerLauncher takes a PlayerSettings&.
        m_tmpdir = std::make_unique<QTemporaryDir>();
        QVERIFY(m_tmpdir->isValid());
        m_config = KSharedConfig::openConfig(
            m_tmpdir->filePath(QStringLiteral("kinemarc")),
            KConfig::SimpleConfig);
        m_settings = std::make_unique<kinema::config::AppSettings>(m_config);
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

    void testCopyMagnetWritesClipboardAndEmitsStatus()
    {
        Stream s;
        s.infoHash = QStringLiteral("0123456789abcdef0123456789abcdef01234567");
        s.releaseName = QStringLiteral("Test.Release.1080p");

        QSignalSpy spy(m_actions.get(), &StreamActions::statusMessage);
        m_actions->copyMagnet(s);

        const auto clip = QGuiApplication::clipboard()->text();
        QVERIFY(clip.startsWith(QLatin1String("magnet:?xt=urn:btih:")));
        QVERIFY(clip.contains(s.infoHash));
        QCOMPARE(spy.count(), 1);
    }

    void testCopyMagnetNoOpWithoutInfoHash()
    {
        Stream s; // empty info hash
        // Clear the clipboard so we can detect a (non-)write.
        QGuiApplication::clipboard()->setText(
            QStringLiteral("__sentinel__"));
        QSignalSpy spy(m_actions.get(), &StreamActions::statusMessage);

        m_actions->copyMagnet(s);

        QCOMPARE(QGuiApplication::clipboard()->text(),
            QStringLiteral("__sentinel__"));
        QCOMPARE(spy.count(), 0);
    }

    void testCopyDirectUrlNoOpWhenEmpty()
    {
        Stream s; // no directUrl
        QGuiApplication::clipboard()->setText(
            QStringLiteral("__sentinel__"));
        QSignalSpy spy(m_actions.get(), &StreamActions::statusMessage);

        m_actions->copyDirectUrl(s);

        QCOMPARE(QGuiApplication::clipboard()->text(),
            QStringLiteral("__sentinel__"));
        QCOMPARE(spy.count(), 0);
    }

    void testCopyDirectUrlWritesClipboard()
    {
        Stream s;
        s.directUrl = QUrl(QStringLiteral("https://example.com/x.mp4"));

        QSignalSpy spy(m_actions.get(), &StreamActions::statusMessage);
        m_actions->copyDirectUrl(s);

        QCOMPARE(QGuiApplication::clipboard()->text(),
            QStringLiteral("https://example.com/x.mp4"));
        QCOMPARE(spy.count(), 1);
    }

    void testPlayWithoutDirectUrlEmitsStatus()
    {
        Stream s; // no directUrl
        QSignalSpy spy(m_actions.get(), &StreamActions::statusMessage);

        kinema::api::PlaybackContext ctx;
        ctx.key.kind = kinema::api::MediaKind::Movie;
        ctx.key.imdbId = QStringLiteral("tt0000001");
        m_actions->play(s, ctx);

        // One status message (the "no direct URL" branch); no
        // launcher call attempted.
        QCOMPARE(spy.count(), 1);
        const auto msg = spy.first().at(0).toString();
        QVERIFY(msg.contains(QStringLiteral("Real-Debrid")));
    }

private:
    std::unique_ptr<QTemporaryDir> m_tmpdir;
    KSharedConfigPtr m_config;
    std::unique_ptr<kinema::config::AppSettings> m_settings;
    std::unique_ptr<PlayerLauncher> m_launcher;
    std::unique_ptr<StreamActions> m_actions;
};

QTEST_MAIN(TstStreamActions)
#include "tst_stream_actions.moc"
