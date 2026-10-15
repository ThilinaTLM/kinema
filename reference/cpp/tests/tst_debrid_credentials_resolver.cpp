// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/DebridCredentialsResolver.h"

#include "TestDoubles.h"
#include "config/DebridSettings.h"
#include "controllers/TokenController.h"
#include "core/persistence/TokenStore.h"
#include "domain/Debrid.h"
#include "domain/DebridCredentials.h"

#include <KConfig>
#include <KConfigGroup>
#include <KSharedConfig>

#include <QTemporaryDir>
#include <QTest>

using namespace kinema;

class TstDebridCredentialsResolver : public QObject
{
    Q_OBJECT

private:
    QTemporaryDir m_tmp;
    KSharedConfigPtr m_config;

private Q_SLOTS:
    void init()
    {
        m_config = KSharedConfig::openConfig(
            m_tmp.filePath(QStringLiteral("kinemarc")),
            KConfig::SimpleConfig);
        m_config->group(QStringLiteral("Debrid")).deleteGroup();
    }

    /// Helper: build a TokenController hooked to a FakeTokenStore and
    /// spin the event loop until the requested provider's load slot
    /// has populated its cache. Returns the controller by value so
    /// callers can pass it to the resolver under test.
    static void primeTokens(controllers::TokenController& tc,
        domain::DebridProvider p)
    {
        if (p == domain::DebridProvider::RealDebrid) {
            tc.refreshRealDebrid();
        } else if (p == domain::DebridProvider::AllDebrid) {
            tc.refreshAllDebrid();
        }
    }

    void noActiveProvider_returnsNone()
    {
        config::DebridSettings settings(m_config);
        // Default = None.
        tests::FakeTokenStore tokens;
        tokens.values[QString::fromLatin1(core::TokenStore::kRealDebridKey)]
            = QStringLiteral("rd-token");
        tokens.values[QString::fromLatin1(core::TokenStore::kAllDebridKey)]
            = QStringLiteral("ad-key");

        controllers::TokenController tc(&tokens, /*tmdb=*/nullptr,
            settings, nullptr, QStringLiteral(""));
        // Even with tokens primed in the store, no active provider
        // means `active()` must collapse to None.
        primeTokens(tc, domain::DebridProvider::RealDebrid);
        primeTokens(tc, domain::DebridProvider::AllDebrid);

        controllers::DebridCredentialsResolver resolver(settings, tc);
        const auto a = resolver.active();
        QCOMPARE(a.provider, domain::DebridProvider::None);
        QVERIFY(a.token.isEmpty());
    }

    void realDebridActive_withToken_returnsCredential()
    {
        config::DebridSettings settings(m_config);
        settings.setActiveProvider(domain::DebridProvider::RealDebrid);
        settings.setRealDebridConfigured(true);
        tests::FakeTokenStore tokens;
        tokens.values[QString::fromLatin1(core::TokenStore::kRealDebridKey)]
            = QStringLiteral("rd-token");

        controllers::TokenController tc(&tokens, /*tmdb=*/nullptr,
            settings, nullptr, QStringLiteral(""));
        tc.refreshRealDebrid();
        // refreshRealDebrid launches an async coroutine; spin the
        // event loop until the cache has caught up.
        QTRY_COMPARE(tc.realDebridToken(),
            QStringLiteral("rd-token"));

        controllers::DebridCredentialsResolver resolver(settings, tc);
        const auto a = resolver.active();
        QCOMPARE(a.provider, domain::DebridProvider::RealDebrid);
        QCOMPARE(a.token, QStringLiteral("rd-token"));
    }

    void allDebridActive_withToken_returnsCredential()
    {
        config::DebridSettings settings(m_config);
        settings.setActiveProvider(domain::DebridProvider::AllDebrid);
        settings.setAllDebridConfigured(true);
        tests::FakeTokenStore tokens;
        tokens.values[QString::fromLatin1(core::TokenStore::kAllDebridKey)]
            = QStringLiteral("ad-key");

        controllers::TokenController tc(&tokens, /*tmdb=*/nullptr,
            settings, nullptr, QStringLiteral(""));
        tc.refreshAllDebrid();
        QTRY_COMPARE(tc.allDebridApiKey(), QStringLiteral("ad-key"));

        controllers::DebridCredentialsResolver resolver(settings, tc);
        const auto a = resolver.active();
        QCOMPARE(a.provider, domain::DebridProvider::AllDebrid);
        QCOMPARE(a.token, QStringLiteral("ad-key"));
    }

    void activeProviderWithEmptyToken_collapsesToNone()
    {
        // User flipped the radio to RD but never saved a token (or
        // wiped it). `realDebridConfigured` is false so the
        // controller never reads anything; resolver must therefore
        // return `{None, {}}`, not `{RealDebrid, ""}`.
        config::DebridSettings settings(m_config);
        settings.setActiveProvider(domain::DebridProvider::RealDebrid);
        // realDebridConfigured stays false → TokenController never
        // reads from the keyring.
        tests::FakeTokenStore tokens;
        controllers::TokenController tc(&tokens, /*tmdb=*/nullptr,
            settings, nullptr, QStringLiteral(""));

        controllers::DebridCredentialsResolver resolver(settings, tc);
        const auto a = resolver.active();
        QCOMPARE(a.provider, domain::DebridProvider::None);
        QVERIFY(a.token.isEmpty());
    }

    void switchingProvider_isPickedUpOnNextActiveCall()
    {
        // The resolver reads settings on every call, so toggling
        // `activeProvider` mid-session is reflected without any
        // explicit refresh.
        config::DebridSettings settings(m_config);
        settings.setRealDebridConfigured(true);
        settings.setAllDebridConfigured(true);

        tests::FakeTokenStore tokens;
        tokens.values[QString::fromLatin1(core::TokenStore::kRealDebridKey)]
            = QStringLiteral("rd-token");
        tokens.values[QString::fromLatin1(core::TokenStore::kAllDebridKey)]
            = QStringLiteral("ad-key");

        controllers::TokenController tc(&tokens, /*tmdb=*/nullptr,
            settings, nullptr, QStringLiteral(""));
        tc.refreshRealDebrid();
        tc.refreshAllDebrid();
        QTRY_COMPARE(tc.realDebridToken(),
            QStringLiteral("rd-token"));
        QTRY_COMPARE(tc.allDebridApiKey(), QStringLiteral("ad-key"));

        controllers::DebridCredentialsResolver resolver(settings, tc);

        settings.setActiveProvider(domain::DebridProvider::RealDebrid);
        QCOMPARE(resolver.active().provider,
            domain::DebridProvider::RealDebrid);
        QCOMPARE(resolver.active().token,
            QStringLiteral("rd-token"));

        settings.setActiveProvider(domain::DebridProvider::AllDebrid);
        QCOMPARE(resolver.active().provider,
            domain::DebridProvider::AllDebrid);
        QCOMPARE(resolver.active().token,
            QStringLiteral("ad-key"));

        settings.setActiveProvider(domain::DebridProvider::None);
        QCOMPARE(resolver.active().provider,
            domain::DebridProvider::None);
    }
};

QTEST_GUILESS_MAIN(TstDebridCredentialsResolver)
#include "tst_debrid_credentials_resolver.moc"
