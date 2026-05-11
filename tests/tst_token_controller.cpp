// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/TokenController.h"

#include "api/TmdbClient.h"
#include "config/AppSettings.h"
#include "config/DebridSettings.h"
#include "core/persistence/TokenStore.h"
#include "TestDoubles.h"

#include <KConfig>
#include <KSharedConfig>

#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>

using kinema::api::TmdbClient;
using kinema::config::AppSettings;
using kinema::controllers::TokenController;
using kinema::core::TokenStore;
using kinema::tests::FakeTokenStore;
using kinema::tests::drainEvents;

namespace {

struct Fixture {
    QTemporaryDir tmpDir;
    KSharedConfigPtr config;
    AppSettings settings;
    FakeTokenStore tokens;
    TmdbClient tmdb { nullptr };

    Fixture()
        : config(KSharedConfig::openConfig(
            tmpDir.filePath(QStringLiteral("kinemarc")),
            KConfig::SimpleConfig))
        , settings(config)
    {
    }
};

} // namespace

class TstTokenController : public QObject
{
    Q_OBJECT

private Q_SLOTS:
    void testLoadAllSkipsRealDebridReadWhenNotConfigured()
    {
        Fixture f;
        f.tokens.values.insert(QString::fromLatin1(TokenStore::kTmdbKey),
            QStringLiteral("tmdb-user"));
        f.tokens.values.insert(QString::fromLatin1(TokenStore::kRealDebridKey),
            QStringLiteral("rd-token"));

        TokenController controller(
            &f.tokens, &f.tmdb, f.settings.debrid(), nullptr,
            QStringLiteral("compiled-default"));
        controller.loadAll();
        drainEvents();

        // RD must NOT be read when no token has been saved (the
        // configured flag flips to true via Save in the settings VM).
        QVERIFY(!f.tokens.readKeys.contains(
            QString::fromLatin1(TokenStore::kRealDebridKey)));
        QVERIFY(f.tokens.readKeys.contains(
            QString::fromLatin1(TokenStore::kTmdbKey)));
        QVERIFY(controller.realDebridToken().isEmpty());
        QCOMPARE(controller.tmdbToken(), QStringLiteral("tmdb-user"));
    }

    void testLoadAllReadsRealDebridWhenConfigured()
    {
        Fixture f;
        f.settings.debrid().setRealDebridConfigured(true);
        f.tokens.values.insert(QString::fromLatin1(TokenStore::kTmdbKey),
            QStringLiteral("tmdb-user"));
        f.tokens.values.insert(QString::fromLatin1(TokenStore::kRealDebridKey),
            QStringLiteral("rd-token"));

        TokenController controller(
            &f.tokens, &f.tmdb, f.settings.debrid(), nullptr,
            QStringLiteral("compiled-default"));
        controller.loadAll();
        drainEvents();

        QVERIFY(f.tokens.readKeys.contains(
            QString::fromLatin1(TokenStore::kRealDebridKey)));
        QVERIFY(f.tokens.readKeys.contains(
            QString::fromLatin1(TokenStore::kTmdbKey)));
        QCOMPARE(controller.realDebridToken(), QStringLiteral("rd-token"));
    }

    void testLoadAllReadsAllDebridWhenConfigured()
    {
        Fixture f;
        f.settings.debrid().setAllDebridConfigured(true);
        f.tokens.values.insert(QString::fromLatin1(TokenStore::kAllDebridKey),
            QStringLiteral("ad-key"));

        TokenController controller(
            &f.tokens, &f.tmdb, f.settings.debrid(), nullptr,
            QStringLiteral(""));
        controller.loadAll();
        drainEvents();

        QVERIFY(f.tokens.readKeys.contains(
            QString::fromLatin1(TokenStore::kAllDebridKey)));
        QCOMPARE(controller.allDebridApiKey(), QStringLiteral("ad-key"));
    }

    void testLoadAllSkipsAllDebridReadWhenNotConfigured()
    {
        Fixture f;
        f.tokens.values.insert(QString::fromLatin1(TokenStore::kAllDebridKey),
            QStringLiteral("ad-key"));

        TokenController controller(
            &f.tokens, &f.tmdb, f.settings.debrid(), nullptr,
            QStringLiteral(""));
        controller.loadAll();
        drainEvents();

        QVERIFY(!f.tokens.readKeys.contains(
            QString::fromLatin1(TokenStore::kAllDebridKey)));
        QVERIFY(controller.allDebridApiKey().isEmpty());
    }

    void testTmdbTokenPrecedenceAndClientState()
    {
        Fixture f;
        f.tokens.values.insert(QString::fromLatin1(TokenStore::kTmdbKey),
            QStringLiteral("user-token"));

        TokenController controller(
            &f.tokens, &f.tmdb, f.settings.debrid(), nullptr,
            QStringLiteral("compiled-default"));
        controller.loadAll();
        drainEvents();

        QCOMPARE(controller.tmdbToken(), QStringLiteral("user-token"));
        QVERIFY(f.tmdb.hasToken());

        f.tokens.values.remove(QString::fromLatin1(TokenStore::kTmdbKey));
        controller.refreshTmdb();
        drainEvents();

        QCOMPARE(controller.tmdbToken(), QStringLiteral("compiled-default"));
        QVERIFY(f.tmdb.hasToken());

        TmdbClient tmdbNoDefault(nullptr);
        TokenController noDefault(
            &f.tokens, &tmdbNoDefault, f.settings.debrid(), nullptr,
            QStringLiteral(""));
        noDefault.refreshTmdb();
        drainEvents();

        QVERIFY(noDefault.tmdbToken().isEmpty());
        QVERIFY(!tmdbNoDefault.hasToken());
    }

    void testSignalsOnlyEmitWhenEffectiveValueChanges()
    {
        Fixture f;
        f.settings.debrid().setRealDebridConfigured(true);
        f.tokens.values.insert(QString::fromLatin1(TokenStore::kTmdbKey),
            QStringLiteral("tmdb-a"));
        f.tokens.values.insert(QString::fromLatin1(TokenStore::kRealDebridKey),
            QStringLiteral("rd-a"));

        TokenController controller(
            &f.tokens, &f.tmdb, f.settings.debrid(), nullptr,
            QStringLiteral("compiled-default"));
        QSignalSpy rdSpy(&controller, &TokenController::realDebridTokenChanged);
        QSignalSpy tmdbSpy(&controller, &TokenController::tmdbTokenChanged);

        controller.loadAll();
        drainEvents();
        QCOMPARE(rdSpy.count(), 1);
        QCOMPARE(tmdbSpy.count(), 1);

        controller.loadAll();
        drainEvents();
        QCOMPARE(rdSpy.count(), 1);
        QCOMPARE(tmdbSpy.count(), 1);

        f.tokens.values.insert(QString::fromLatin1(TokenStore::kTmdbKey),
            QStringLiteral("tmdb-b"));
        controller.refreshTmdb();
        drainEvents();
        QCOMPARE(tmdbSpy.count(), 2);
        QCOMPARE(rdSpy.count(), 1);
    }

    void testRefreshAllDebridAfterRemove_clearsEffectiveKey()
    {
        Fixture f;
        f.settings.debrid().setAllDebridConfigured(true);
        f.tokens.values.insert(QString::fromLatin1(TokenStore::kAllDebridKey),
            QStringLiteral("ad-a"));

        TokenController controller(
            &f.tokens, &f.tmdb, f.settings.debrid(), nullptr,
            QStringLiteral(""));
        QSignalSpy adSpy(&controller, &TokenController::allDebridApiKeyChanged);

        controller.loadAll();
        drainEvents();
        QCOMPARE(controller.allDebridApiKey(), QStringLiteral("ad-a"));
        QCOMPARE(adSpy.count(), 1);

        // Settings page Remove path: clears the keyring and flips
        // the configured flag.
        f.tokens.values.remove(QString::fromLatin1(TokenStore::kAllDebridKey));
        f.settings.debrid().setAllDebridConfigured(false);
        controller.refreshAllDebrid();
        drainEvents();

        QVERIFY(controller.allDebridApiKey().isEmpty());
        QCOMPARE(adSpy.count(), 2);
        QCOMPARE(adSpy.last().at(0).toString(), QString {});
    }

    void testReadFailureLeavesSafeState()
    {
        Fixture f;
        f.settings.debrid().setRealDebridConfigured(true);
        f.tokens.failingReads = {
            QString::fromLatin1(TokenStore::kTmdbKey),
            QString::fromLatin1(TokenStore::kRealDebridKey)
        };

        TokenController controller(
            &f.tokens, &f.tmdb, f.settings.debrid(), nullptr,
            QStringLiteral(""));
        QSignalSpy rdSpy(&controller, &TokenController::realDebridTokenChanged);
        QSignalSpy tmdbSpy(&controller, &TokenController::tmdbTokenChanged);

        controller.loadAll();
        drainEvents();

        QVERIFY(controller.realDebridToken().isEmpty());
        QVERIFY(controller.tmdbToken().isEmpty());
        QVERIFY(!f.tmdb.hasToken());
        QCOMPARE(rdSpy.count(), 0);
        QCOMPARE(tmdbSpy.count(), 0);
    }
};

QTEST_MAIN(TstTokenController)
#include "tst_token_controller.moc"
