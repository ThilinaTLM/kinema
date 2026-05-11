// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/SettingsViewModels.h"

#include "api/AllDebridClient.h"
#include "api/Indexer.h"
#include "api/IndexerSelector.h"
#include "api/Media.h"
#include "api/OpenSubtitlesClient.h"
#include "api/RealDebridClient.h"
#include "api/Subtitle.h"
#include "api/TmdbClient.h"
#include "config/AppSettings.h"
#include "config/AppearanceSettings.h"
#include "config/CacheSettings.h"
#include "config/DebridSettings.h"
#include "config/FilterSettings.h"
#include "config/IndexerSettings.h"
#include "config/PlayerSettings.h"
#include "config/SearchSettings.h"
#include "config/SubtitleSettings.h"
#include "config/TorrentioSettings.h"
#include "config/TorrentStreamingSettings.h"
#include "core/DateFormat.h"
#include "core/HttpClient.h"
#include "core/HttpError.h"
#include "core/HttpErrorPresenter.h"
#include "core/Language.h"
#include "core/MediaCache.h"
#include "core/Player.h"
#include "core/SubtitleCacheStore.h"
#include "kinema_log_ui.h"

#include <KFormat>
#include "core/TmdbConfig.h"
#include "core/TokenStore.h"
#include "core/TorrentioConfig.h"

#include <KLocalizedString>

#include <QFile>
#include <QSystemTrayIcon>
#include <QTimer>

namespace kinema::ui::qml {

namespace {

// `Kirigami.MessageType` int-equivalents kept local so QML can use
// the small integer literals without dragging Kirigami into C++.
constexpr int kStatusInfo = 0;
constexpr int kStatusPositive = 1;
constexpr int kStatusError = 3;
// (Kirigami's enum has Warning = 2 between Positive and Error; we
//  don't surface warnings in any of these forms.)

int sortModeToIndex(core::torrentio::SortMode m)
{
    switch (m) {
    case core::torrentio::SortMode::Seeders: return 0;
    case core::torrentio::SortMode::Size: return 1;
    case core::torrentio::SortMode::QualitySize: return 2;
    }
    return 0;
}

core::torrentio::SortMode indexToSortMode(int idx)
{
    switch (idx) {
    case 1: return core::torrentio::SortMode::Size;
    case 2: return core::torrentio::SortMode::QualitySize;
    default: return core::torrentio::SortMode::Seeders;
    }
}

int playerKindToIndex(core::player::Kind kind)
{
    switch (kind) {
    case core::player::Kind::Embedded: return 0;
    case core::player::Kind::Mpv: return 1;
    case core::player::Kind::Vlc: return 2;
    case core::player::Kind::Custom: return 3;
    }
    return 1;
}

core::player::Kind indexToPlayerKind(int idx)
{
    switch (idx) {
    case 0: return core::player::Kind::Embedded;
    case 2: return core::player::Kind::Vlc;
    case 3: return core::player::Kind::Custom;
    default: return core::player::Kind::Mpv;
    }
}

} // namespace

// ============================== Application ===============================

GeneralSettingsViewModel::GeneralSettingsViewModel(
    config::SearchSettings& search,
    config::AppearanceSettings& appearance, QObject* parent)
    : QObject(parent)
    , m_search(search)
    , m_appearance(appearance)
{
}

int GeneralSettingsViewModel::defaultSearchKind() const
{
    return m_search.kind() == api::MediaKind::Series ? 1 : 0;
}

bool GeneralSettingsViewModel::closeToTray() const
{
    return m_appearance.closeToTray();
}

bool GeneralSettingsViewModel::trayAvailable() const
{
    return QSystemTrayIcon::isSystemTrayAvailable();
}

void GeneralSettingsViewModel::setDefaultSearchKind(int kind)
{
    const auto next = kind == 1 ? api::MediaKind::Series : api::MediaKind::Movie;
    if (m_search.kind() == next) {
        return;
    }
    m_search.setKind(next);
    Q_EMIT defaultSearchKindChanged();
}

void GeneralSettingsViewModel::setCloseToTray(bool on)
{
    if (m_appearance.closeToTray() == on) {
        return;
    }
    m_appearance.setCloseToTray(on);
    Q_EMIT closeToTrayChanged();
}

// ============================== TMDB ======================================

TmdbSettingsViewModel::TmdbSettingsViewModel(
    core::HttpClient* http, core::TokenStore* tokens, QObject* parent)
    : QObject(parent)
    , m_http(http)
    , m_tokens(tokens)
{
}

bool TmdbSettingsViewModel::hasCompiledDefault() const
{
    return core::kTmdbCompiledDefaultToken != nullptr
        && core::kTmdbCompiledDefaultToken[0] != '\0';
}

void TmdbSettingsViewModel::setToken(const QString& token)
{
    if (m_token == token) {
        return;
    }
    m_token = token;
    Q_EMIT tokenInputChanged();
}

void TmdbSettingsViewModel::load()
{
    auto t = loadTask();
    Q_UNUSED(t);
}

void TmdbSettingsViewModel::testConnection()
{
    auto t = testTask();
    Q_UNUSED(t);
}

void TmdbSettingsViewModel::save()
{
    auto t = saveTask();
    Q_UNUSED(t);
}

void TmdbSettingsViewModel::remove()
{
    auto t = removeTask();
    Q_UNUSED(t);
}

void TmdbSettingsViewModel::setStatus(const QString& message, int kind)
{
    if (m_statusMessage == message && m_statusKind == kind) {
        return;
    }
    m_statusMessage = message;
    m_statusKind = kind;
    Q_EMIT statusChanged();
}

void TmdbSettingsViewModel::setBusy(bool on)
{
    if (m_busy == on) {
        return;
    }
    m_busy = on;
    Q_EMIT busyChanged();
}

QCoro::Task<void> TmdbSettingsViewModel::loadTask()
{
    setBusy(true);
    try {
        const auto existing = co_await m_tokens->read(
            QString::fromLatin1(core::TokenStore::kTmdbKey));
        if (!existing.isEmpty()) {
            setToken(existing);
            if (!m_userTokenSaved) {
                m_userTokenSaved = true;
                Q_EMIT userTokenSavedChanged();
            }
            setStatus(i18nc("@info tmdb settings status",
                "Using your token."), kStatusPositive);
        } else if (hasCompiledDefault()) {
            setStatus(i18nc("@info tmdb settings status",
                "Using the bundled token."), kStatusInfo);
        } else {
            setStatus(i18nc("@info tmdb settings status",
                "No token configured. Discover is disabled."),
                kStatusError);
        }
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "tmdb settings/load"),
            kStatusError);
    }
    setBusy(false);
}

QCoro::Task<void> TmdbSettingsViewModel::testTask()
{
    QString token = m_token.trimmed();
    if (token.isEmpty()) {
        if (m_userTokenSaved) {
            try {
                token = co_await m_tokens->read(
                    QString::fromLatin1(core::TokenStore::kTmdbKey));
            } catch (const std::exception& e) {
                setStatus(
                    core::describeError(e,
                        "tmdb settings/read saved token"),
                    kStatusError);
                co_return;
            }
        }
        if (token.isEmpty() && hasCompiledDefault()) {
            token
                = QString::fromLatin1(core::kTmdbCompiledDefaultToken);
        }
    }
    if (token.isEmpty()) {
        setStatus(i18nc("@info tmdb settings status",
            "No token to test. Paste one above or build with "
            "KINEMA_TMDB_DEFAULT_TOKEN."),
            kStatusError);
        co_return;
    }

    setBusy(true);
    setStatus(i18nc("@info tmdb settings status, in progress",
        "Testing TMDB credentials…"), kStatusInfo);
    api::TmdbClient client(m_http);
    client.setToken(token);
    try {
        co_await client.testAuth();
        setStatus(i18nc("@info tmdb settings status",
            "Connected to TMDB."), kStatusPositive);
    } catch (const std::exception& e) {
        if (const auto* he = core::asHttpError(e);
            he
            && (he->httpStatus() == 401 || he->httpStatus() == 403)) {
            setStatus(i18nc("@info tmdb settings status",
                "TMDB rejected the token (HTTP %1).",
                he->httpStatus()),
                kStatusError);
        } else {
            setStatus(core::describeError(e,
                          "tmdb settings/test auth"),
                kStatusError);
        }
    }
    setBusy(false);
}

QCoro::Task<void> TmdbSettingsViewModel::saveTask()
{
    const auto token = m_token.trimmed();
    if (token.isEmpty()) {
        co_return;
    }
    setBusy(true);
    try {
        co_await m_tokens->write(
            QString::fromLatin1(core::TokenStore::kTmdbKey), token);
        if (!m_userTokenSaved) {
            m_userTokenSaved = true;
            Q_EMIT userTokenSavedChanged();
        }
        Q_EMIT tokenChanged(token);
        setStatus(i18nc("@info tmdb settings status",
            "Token saved to keyring. Using your token."),
            kStatusPositive);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "tmdb settings/save"),
            kStatusError);
    }
    setBusy(false);
}

QCoro::Task<void> TmdbSettingsViewModel::removeTask()
{
    setBusy(true);
    try {
        co_await m_tokens->remove(
            QString::fromLatin1(core::TokenStore::kTmdbKey));
        m_token.clear();
        Q_EMIT tokenInputChanged();
        if (m_userTokenSaved) {
            m_userTokenSaved = false;
            Q_EMIT userTokenSavedChanged();
        }
        Q_EMIT tokenChanged(QString {});
        setStatus(hasCompiledDefault()
                ? i18nc("@info tmdb settings status",
                      "Token removed. Falling back to bundled "
                      "token.")
                : i18nc("@info tmdb settings status",
                      "Token removed. No token configured."),
            kStatusInfo);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "tmdb settings/remove"),
            kStatusError);
    }
    setBusy(false);
}

// ============================== Debrid: Real-Debrid section ==============

RealDebridSectionViewModel::RealDebridSectionViewModel(
    core::HttpClient* http, core::TokenStore* tokens,
    config::DebridSettings& settings, QObject* parent)
    : QObject(parent)
    , m_http(http)
    , m_tokens(tokens)
    , m_settings(settings)
{
}

bool RealDebridSectionViewModel::tokenSaved() const
{
    return m_settings.realDebridConfigured();
}

void RealDebridSectionViewModel::setToken(const QString& token)
{
    if (m_token == token) {
        return;
    }
    m_token = token;
    Q_EMIT tokenInputChanged();
}

void RealDebridSectionViewModel::load()
{
    auto t = loadTask();
    Q_UNUSED(t);
}
void RealDebridSectionViewModel::testConnection()
{
    auto t = testTask();
    Q_UNUSED(t);
}
void RealDebridSectionViewModel::save()
{
    auto t = saveTask();
    Q_UNUSED(t);
}
void RealDebridSectionViewModel::remove()
{
    auto t = removeTask();
    Q_UNUSED(t);
}

void RealDebridSectionViewModel::setStatus(const QString& message, int kind)
{
    if (m_statusMessage == message && m_statusKind == kind) {
        return;
    }
    m_statusMessage = message;
    m_statusKind = kind;
    Q_EMIT statusChanged();
}

void RealDebridSectionViewModel::setBusy(bool on)
{
    if (m_busy == on) {
        return;
    }
    m_busy = on;
    Q_EMIT busyChanged();
}

QCoro::Task<void> RealDebridSectionViewModel::loadTask()
{
    setBusy(true);
    try {
        const auto existing = co_await m_tokens->read(
            QString::fromLatin1(core::TokenStore::kRealDebridKey));
        if (!existing.isEmpty()) {
            setToken(existing);
        }
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "rd settings/load"),
            kStatusError);
    }
    setBusy(false);
}

QCoro::Task<void> RealDebridSectionViewModel::testTask()
{
    const auto token = m_token.trimmed();
    if (token.isEmpty()) {
        co_return;
    }
    setBusy(true);
    setStatus(i18nc("@info rd settings status, in progress",
        "Testing Real-Debrid token…"), kStatusInfo);
    api::RealDebridClient client(m_http);
    client.setToken(token);
    try {
        const auto user = co_await client.user();
        QString msg = i18nc("@info rd settings status",
            "Signed in as %1 (%2)",
            user.username.isEmpty() ? QStringLiteral("—")
                                    : user.username,
            user.type.isEmpty() ? QStringLiteral("?") : user.type);
        if (user.premiumUntil) {
            msg += QLatin1Char('\n')
                + i18nc("@info rd settings status premium expiry",
                    "Premium until: %1",
                    core::formatReleaseDate(*user.premiumUntil));
        }
        setStatus(msg, kStatusPositive);
    } catch (const std::exception& e) {
        if (const auto* he = core::asHttpError(e);
            he
            && (he->httpStatus() == 401 || he->httpStatus() == 403)) {
            setStatus(i18nc("@info rd settings status",
                "Real-Debrid rejected the token (HTTP %1).",
                he->httpStatus()),
                kStatusError);
        } else {
            setStatus(core::describeError(e, "rd settings/test"),
                kStatusError);
        }
    }
    setBusy(false);
}

QCoro::Task<void> RealDebridSectionViewModel::saveTask()
{
    const auto token = m_token.trimmed();
    if (token.isEmpty()) {
        co_return;
    }
    setBusy(true);
    try {
        co_await m_tokens->write(
            QString::fromLatin1(core::TokenStore::kRealDebridKey),
            token);
        m_settings.setRealDebridConfigured(true);
        Q_EMIT tokenSavedChanged();
        Q_EMIT tokenChanged(token);
        setStatus(i18nc("@info rd settings status",
            "Token saved to keyring."), kStatusPositive);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "rd settings/save"),
            kStatusError);
    }
    setBusy(false);
}

QCoro::Task<void> RealDebridSectionViewModel::removeTask()
{
    setBusy(true);
    try {
        co_await m_tokens->remove(
            QString::fromLatin1(core::TokenStore::kRealDebridKey));
        m_settings.setRealDebridConfigured(false);
        m_token.clear();
        Q_EMIT tokenInputChanged();
        Q_EMIT tokenSavedChanged();
        Q_EMIT tokenChanged(QString {});
        setStatus(i18nc("@info rd settings status",
            "Token removed from keyring."), kStatusInfo);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "rd settings/remove"),
            kStatusError);
    }
    setBusy(false);
}

// ============================== Debrid: AllDebrid section ===============

AllDebridSectionViewModel::AllDebridSectionViewModel(
    core::HttpClient* http, core::TokenStore* tokens,
    config::DebridSettings& settings, QObject* parent)
    : QObject(parent)
    , m_http(http)
    , m_tokens(tokens)
    , m_settings(settings)
{
}

bool AllDebridSectionViewModel::apiKeySaved() const
{
    return m_settings.allDebridConfigured();
}

void AllDebridSectionViewModel::setApiKey(const QString& apiKey)
{
    if (m_apiKey == apiKey) {
        return;
    }
    m_apiKey = apiKey;
    Q_EMIT apiKeyInputChanged();
}

void AllDebridSectionViewModel::load()
{
    auto t = loadTask();
    Q_UNUSED(t);
}
void AllDebridSectionViewModel::testConnection()
{
    auto t = testTask();
    Q_UNUSED(t);
}
void AllDebridSectionViewModel::save()
{
    auto t = saveTask();
    Q_UNUSED(t);
}
void AllDebridSectionViewModel::remove()
{
    auto t = removeTask();
    Q_UNUSED(t);
}

void AllDebridSectionViewModel::setStatus(const QString& message, int kind)
{
    if (m_statusMessage == message && m_statusKind == kind) {
        return;
    }
    m_statusMessage = message;
    m_statusKind = kind;
    Q_EMIT statusChanged();
}

void AllDebridSectionViewModel::setBusy(bool on)
{
    if (m_busy == on) {
        return;
    }
    m_busy = on;
    Q_EMIT busyChanged();
}

QCoro::Task<void> AllDebridSectionViewModel::loadTask()
{
    setBusy(true);
    try {
        const auto existing = co_await m_tokens->read(
            QString::fromLatin1(core::TokenStore::kAllDebridKey));
        if (!existing.isEmpty()) {
            setApiKey(existing);
        }
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "ad settings/load"),
            kStatusError);
    }
    setBusy(false);
}

QCoro::Task<void> AllDebridSectionViewModel::testTask()
{
    const auto apiKey = m_apiKey.trimmed();
    if (apiKey.isEmpty()) {
        co_return;
    }
    setBusy(true);
    setStatus(i18nc("@info ad settings status, in progress",
        "Testing AllDebrid API key…"), kStatusInfo);
    api::AllDebridClient client(m_http);
    client.setApiKey(apiKey);
    try {
        const auto user = co_await client.user();
        const auto plan = user.isPremium
            ? (user.isTrial
                ? i18nc("@info ad plan label", "trial")
                : i18nc("@info ad plan label", "premium"))
            : i18nc("@info ad plan label", "free");
        QString msg = i18nc("@info ad settings status",
            "Signed in as %1 (%2)",
            user.username.isEmpty() ? QStringLiteral("—")
                                    : user.username,
            plan);
        if (user.premiumUntil) {
            msg += QLatin1Char('\n')
                + i18nc("@info ad settings status premium expiry",
                    "Premium until: %1",
                    core::formatReleaseDate(*user.premiumUntil));
        }
        setStatus(msg, kStatusPositive);
    } catch (const std::exception& e) {
        if (const auto* he = core::asHttpError(e);
            he
            && (he->httpStatus() == 401 || he->httpStatus() == 403)) {
            setStatus(i18nc("@info ad settings status",
                "AllDebrid rejected the API key (HTTP %1).",
                he->httpStatus()),
                kStatusError);
        } else {
            setStatus(core::describeError(e, "ad settings/test"),
                kStatusError);
        }
    }
    setBusy(false);
}

QCoro::Task<void> AllDebridSectionViewModel::saveTask()
{
    const auto apiKey = m_apiKey.trimmed();
    if (apiKey.isEmpty()) {
        co_return;
    }
    setBusy(true);
    try {
        co_await m_tokens->write(
            QString::fromLatin1(core::TokenStore::kAllDebridKey),
            apiKey);
        m_settings.setAllDebridConfigured(true);
        Q_EMIT apiKeySavedChanged();
        Q_EMIT apiKeyChanged(apiKey);
        setStatus(i18nc("@info ad settings status",
            "API key saved to keyring."), kStatusPositive);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "ad settings/save"),
            kStatusError);
    }
    setBusy(false);
}

QCoro::Task<void> AllDebridSectionViewModel::removeTask()
{
    setBusy(true);
    try {
        co_await m_tokens->remove(
            QString::fromLatin1(core::TokenStore::kAllDebridKey));
        m_settings.setAllDebridConfigured(false);
        m_apiKey.clear();
        Q_EMIT apiKeyInputChanged();
        Q_EMIT apiKeySavedChanged();
        Q_EMIT apiKeyChanged(QString {});
        setStatus(i18nc("@info ad settings status",
            "API key removed from keyring."), kStatusInfo);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "ad settings/remove"),
            kStatusError);
    }
    setBusy(false);
}

// ============================== Debrid parent VM =========================

DebridSettingsViewModel::DebridSettingsViewModel(
    core::HttpClient* http, core::TokenStore* tokens,
    config::DebridSettings& settings, QObject* parent)
    : QObject(parent)
    , m_settings(settings)
    , m_rd(new RealDebridSectionViewModel(http, tokens, settings, this))
    , m_ad(new AllDebridSectionViewModel(http, tokens, settings, this))
{
    connect(&m_settings, &config::DebridSettings::activeProviderChanged,
        this, [this](api::DebridProvider) {
            Q_EMIT activeProviderChanged();
        });
}

int DebridSettingsViewModel::activeProvider() const
{
    return static_cast<int>(m_settings.activeProvider());
}

void DebridSettingsViewModel::setActiveProvider(int provider)
{
    auto value = api::DebridProvider::None;
    switch (provider) {
    case static_cast<int>(api::DebridProvider::RealDebrid):
        value = api::DebridProvider::RealDebrid;
        break;
    case static_cast<int>(api::DebridProvider::AllDebrid):
        value = api::DebridProvider::AllDebrid;
        break;
    default:
        break;
    }
    m_settings.setActiveProvider(value);
}

// ============================== Indexers: Torrentio section ==============

TorrentioSectionViewModel::TorrentioSectionViewModel(
    api::IndexerSelector* indexers,
    config::TorrentioSettings& settings, QObject* parent)
    : QObject(parent)
    , m_indexers(indexers)
    , m_settings(settings)
{
    connect(&m_settings, &config::TorrentioSettings::defaultSortChanged,
        this, &TorrentioSectionViewModel::defaultSortChanged);
    connect(&m_settings, &config::TorrentioSettings::baseUrlChanged,
        this, &TorrentioSectionViewModel::baseUrlChanged);
}

int TorrentioSectionViewModel::defaultSort() const
{
    return sortModeToIndex(m_settings.defaultSort());
}

QString TorrentioSectionViewModel::baseUrl() const
{
    return m_settings.baseUrl();
}

QString TorrentioSectionViewModel::defaultBaseUrlString() const
{
    return config::TorrentioSettings::defaultBaseUrl();
}

void TorrentioSectionViewModel::setDefaultSort(int sort)
{
    const auto next = indexToSortMode(sort);
    if (m_settings.defaultSort() == next) {
        return;
    }
    m_settings.setDefaultSort(next);
    // settings emits defaultSortChanged → we re-emit via connect.
}

void TorrentioSectionViewModel::setBaseUrl(const QString& url)
{
    m_settings.setBaseUrl(url);
}

void TorrentioSectionViewModel::resetBaseUrl()
{
    m_settings.setBaseUrl(config::TorrentioSettings::defaultBaseUrl());
}

void TorrentioSectionViewModel::testConnection()
{
    auto t = testTask();
    Q_UNUSED(t);
}

void TorrentioSectionViewModel::setStatus(const QString& message, int kind)
{
    if (m_statusMessage == message && m_statusKind == kind) {
        return;
    }
    m_statusMessage = message;
    m_statusKind = kind;
    Q_EMIT statusChanged();
}

void TorrentioSectionViewModel::setBusy(bool on)
{
    if (m_busy == on) {
        return;
    }
    m_busy = on;
    Q_EMIT busyChanged();
}

QCoro::Task<void> TorrentioSectionViewModel::testTask()
{
    auto* indexer = m_indexers
        ? m_indexers->find(api::IndexerKind::Torrentio)
        : nullptr;
    if (!indexer) {
        setStatus(i18nc("@info indexer settings status",
            "Torrentio is not registered."), kStatusError);
        co_return;
    }
    setBusy(true);
    setStatus(i18nc("@info indexer settings status, in progress",
        "Probing Torrentio\u2026"), kStatusInfo);
    const bool ok = co_await indexer->testConnection();
    if (ok) {
        setStatus(i18nc("@info indexer settings status",
            "Torrentio is reachable."), kStatusPositive);
    } else {
        setStatus(i18nc("@info indexer settings status",
            "Torrentio did not respond. Check the base URL or try again later."),
            kStatusError);
    }
    setBusy(false);
}

// ============================== Indexers: Peerflix section ===============

PeerflixSectionViewModel::PeerflixSectionViewModel(
    api::IndexerSelector* indexers,
    config::PeerflixSettings& settings, QObject* parent)
    : QObject(parent)
    , m_indexers(indexers)
    , m_settings(settings)
{
    connect(&m_settings, &config::PeerflixSettings::baseUrlChanged,
        this, &PeerflixSectionViewModel::baseUrlChanged);
}

QString PeerflixSectionViewModel::baseUrl() const
{
    return m_settings.baseUrl();
}

QString PeerflixSectionViewModel::defaultBaseUrlString() const
{
    return config::PeerflixSettings::defaultBaseUrl();
}

void PeerflixSectionViewModel::setBaseUrl(const QString& url)
{
    m_settings.setBaseUrl(url);
}

void PeerflixSectionViewModel::resetBaseUrl()
{
    m_settings.setBaseUrl(config::PeerflixSettings::defaultBaseUrl());
}

void PeerflixSectionViewModel::testConnection()
{
    auto t = testTask();
    Q_UNUSED(t);
}

void PeerflixSectionViewModel::setStatus(const QString& message, int kind)
{
    if (m_statusMessage == message && m_statusKind == kind) {
        return;
    }
    m_statusMessage = message;
    m_statusKind = kind;
    Q_EMIT statusChanged();
}

void PeerflixSectionViewModel::setBusy(bool on)
{
    if (m_busy == on) {
        return;
    }
    m_busy = on;
    Q_EMIT busyChanged();
}

QCoro::Task<void> PeerflixSectionViewModel::testTask()
{
    auto* indexer = m_indexers
        ? m_indexers->find(api::IndexerKind::Peerflix)
        : nullptr;
    if (!indexer) {
        setStatus(i18nc("@info indexer settings status",
            "Peerflix is not registered."), kStatusError);
        co_return;
    }
    setBusy(true);
    setStatus(i18nc("@info indexer settings status, in progress",
        "Probing Peerflix\u2026"), kStatusInfo);
    const bool ok = co_await indexer->testConnection();
    if (ok) {
        setStatus(i18nc("@info indexer settings status",
            "Peerflix is reachable."), kStatusPositive);
    } else {
        setStatus(i18nc("@info indexer settings status",
            "Peerflix did not respond. Check the base URL or try again later."),
            kStatusError);
    }
    setBusy(false);
}

// ============================== Indexers: parent VM ======================

IndexerSettingsViewModel::IndexerSettingsViewModel(
    api::IndexerSelector* indexers,
    config::IndexerSettings& indexerSettings,
    config::TorrentioSettings& torrentioSettings,
    config::PeerflixSettings& peerflixSettings,
    QObject* parent)
    : QObject(parent)
    , m_settings(indexerSettings)
    , m_torrentio(new TorrentioSectionViewModel(indexers,
          torrentioSettings, this))
    , m_peerflix(new PeerflixSectionViewModel(indexers,
          peerflixSettings, this))
{
    connect(&m_settings, &config::IndexerSettings::activeIndexerChanged,
        this, [this](api::IndexerKind) {
            Q_EMIT activeIndexerChanged();
        });
}

int IndexerSettingsViewModel::activeIndexer() const
{
    return static_cast<int>(m_settings.activeIndexer());
}

void IndexerSettingsViewModel::setActiveIndexer(int kind)
{
    auto value = api::IndexerKind::Torrentio;
    switch (kind) {
    case static_cast<int>(api::IndexerKind::Peerflix):
        value = api::IndexerKind::Peerflix;
        break;
    case static_cast<int>(api::IndexerKind::Torrentio):
    default:
        value = api::IndexerKind::Torrentio;
        break;
    }
    m_settings.setActiveIndexer(value);
}

// ============================== Streams (indexer-agnostic filters) =======

StreamsSettingsViewModel::StreamsSettingsViewModel(
    config::FilterSettings& settings, QObject* parent)
    : QObject(parent)
    , m_settings(settings)
{
}

QStringList StreamsSettingsViewModel::excludedResolutions() const
{
    return m_settings.excludedResolutions();
}
QStringList StreamsSettingsViewModel::excludedCategories() const
{
    return m_settings.excludedCategories();
}
QString StreamsSettingsViewModel::blocklistText() const
{
    return m_settings.keywordBlocklist().join(QLatin1Char('\n'));
}

QVariantList StreamsSettingsViewModel::resolutionOptions() const
{
    QVariantList out;
    const QList<std::pair<QString, QString>> opts {
        { QStringLiteral("4k"), i18nc("@option resolution",
                                    "4K (2160p & 1440p)") },
        { QStringLiteral("1080p"), i18nc("@option resolution", "1080p") },
        { QStringLiteral("720p"), i18nc("@option resolution", "720p") },
        { QStringLiteral("480p"), i18nc("@option resolution", "480p") },
        { QStringLiteral("other"), i18nc("@option resolution",
                                       "Other / unknown") },
    };
    for (const auto& [token, label] : opts) {
        QVariantMap row;
        row.insert(QStringLiteral("token"), token);
        row.insert(QStringLiteral("label"), label);
        out.append(row);
    }
    return out;
}

QVariantList StreamsSettingsViewModel::categoryOptions() const
{
    QVariantList out;
    const QList<std::pair<QString, QString>> opts {
        { QStringLiteral("cam"), i18nc("@option variant", "CAM") },
        { QStringLiteral("scr"), i18nc("@option variant", "Screener") },
        { QStringLiteral("threed"), i18nc("@option variant", "3D") },
        { QStringLiteral("hdr"), i18nc("@option variant", "HDR") },
        { QStringLiteral("hdr10plus"), i18nc("@option variant", "HDR10+") },
        { QStringLiteral("dolbyvision"), i18nc("@option variant",
                                              "Dolby Vision") },
        { QStringLiteral("nonen"), i18nc("@option variant",
                                       "Non-English") },
        { QStringLiteral("unknown"), i18nc("@option variant",
                                         "Unknown quality") },
        { QStringLiteral("brremux"), i18nc("@option variant",
                                         "BluRay REMUX") },
    };
    for (const auto& [token, label] : opts) {
        QVariantMap row;
        row.insert(QStringLiteral("token"), token);
        row.insert(QStringLiteral("label"), label);
        out.append(row);
    }
    return out;
}

void StreamsSettingsViewModel::setExcludedResolutions(
    const QStringList& tokens)
{
    if (m_settings.excludedResolutions() == tokens) {
        return;
    }
    m_settings.setExcludedResolutions(tokens);
    Q_EMIT excludedResolutionsChanged();
}

void StreamsSettingsViewModel::setExcludedCategories(
    const QStringList& tokens)
{
    if (m_settings.excludedCategories() == tokens) {
        return;
    }
    m_settings.setExcludedCategories(tokens);
    Q_EMIT excludedCategoriesChanged();
}

void StreamsSettingsViewModel::setBlocklistText(const QString& text)
{
    QStringList keywords
        = text.split(QLatin1Char('\n'), Qt::SkipEmptyParts);
    for (auto& k : keywords) {
        k = k.trimmed();
    }
    if (m_settings.keywordBlocklist() == keywords) {
        return;
    }
    m_settings.setKeywordBlocklist(keywords);
    Q_EMIT blocklistChanged();
}

void StreamsSettingsViewModel::toggleResolution(const QString& token,
    bool on)
{
    auto next = excludedResolutions();
    const bool present = next.contains(token);
    if (on && !present) {
        next.append(token);
    } else if (!on && present) {
        next.removeAll(token);
    } else {
        return;
    }
    setExcludedResolutions(next);
}

void StreamsSettingsViewModel::toggleCategory(const QString& token,
    bool on)
{
    auto next = excludedCategories();
    const bool present = next.contains(token);
    if (on && !present) {
        next.append(token);
    } else if (!on && present) {
        next.removeAll(token);
    } else {
        return;
    }
    setExcludedCategories(next);
}

bool StreamsSettingsViewModel::resolutionExcluded(
    const QString& token) const
{
    return excludedResolutions().contains(token);
}

bool StreamsSettingsViewModel::categoryExcluded(
    const QString& token) const
{
    return excludedCategories().contains(token);
}

// ============================== Player ====================================

PlayerSettingsViewModel::PlayerSettingsViewModel(
    config::PlayerSettings& settings, QObject* parent)
    : QObject(parent)
    , m_settings(settings)
{
}

int PlayerSettingsViewModel::preferredPlayer() const
{
    return playerKindToIndex(m_settings.preferred());
}
QString PlayerSettingsViewModel::customCommand() const
{
    return m_settings.customCommand();
}
bool PlayerSettingsViewModel::embeddedAvailable() const
{
    return core::player::isAvailable(core::player::Kind::Embedded);
}
bool PlayerSettingsViewModel::mpvAvailable() const
{
    return core::player::isAvailable(core::player::Kind::Mpv);
}
bool PlayerSettingsViewModel::vlcAvailable() const
{
    return core::player::isAvailable(core::player::Kind::Vlc);
}
bool PlayerSettingsViewModel::hardwareDecoding() const
{
    return m_settings.hardwareDecoding();
}
QString PlayerSettingsViewModel::preferredAudioLang() const
{
    return m_settings.preferredAudioLang();
}
QString PlayerSettingsViewModel::preferredSubtitleLang() const
{
    return m_settings.preferredSubtitleLang();
}
bool PlayerSettingsViewModel::skipIntroChapters() const
{
    return m_settings.skipIntroChapters();
}
int PlayerSettingsViewModel::resumePromptThresholdSec() const
{
    return m_settings.resumePromptThresholdSec();
}

void PlayerSettingsViewModel::setPreferredPlayer(int kind)
{
    const auto next = indexToPlayerKind(kind);
    if (m_settings.preferred() == next) {
        return;
    }
    m_settings.setPreferred(next);
    Q_EMIT preferredPlayerChanged();
}
void PlayerSettingsViewModel::setCustomCommand(const QString& cmd)
{
    if (m_settings.customCommand() == cmd) {
        return;
    }
    m_settings.setCustomCommand(cmd);
    Q_EMIT customCommandChanged();
}
void PlayerSettingsViewModel::setHardwareDecoding(bool on)
{
    if (m_settings.hardwareDecoding() == on) {
        return;
    }
    m_settings.setHardwareDecoding(on);
    Q_EMIT hardwareDecodingChanged();
}
void PlayerSettingsViewModel::setPreferredAudioLang(const QString& v)
{
    if (m_settings.preferredAudioLang() == v) {
        return;
    }
    m_settings.setPreferredAudioLang(v);
    Q_EMIT preferredAudioLangChanged();
}
void PlayerSettingsViewModel::setPreferredSubtitleLang(const QString& v)
{
    if (m_settings.preferredSubtitleLang() == v) {
        return;
    }
    m_settings.setPreferredSubtitleLang(v);
    Q_EMIT preferredSubtitleLangChanged();
}
void PlayerSettingsViewModel::setSkipIntroChapters(bool on)
{
    if (m_settings.skipIntroChapters() == on) {
        return;
    }
    m_settings.setSkipIntroChapters(on);
    Q_EMIT skipIntroChaptersChanged();
}
void PlayerSettingsViewModel::setResumePromptThresholdSec(int v)
{
    if (m_settings.resumePromptThresholdSec() == v) {
        return;
    }
    m_settings.setResumePromptThresholdSec(v);
    Q_EMIT resumePromptThresholdSecChanged();
}

// ============================== Subtitles =================================

SubtitlesSettingsViewModel::SubtitlesSettingsViewModel(
    core::HttpClient* http, core::TokenStore* tokens,
    config::SubtitleSettings& subtitleSettings,
    config::CacheSettings& cacheSettings,
    core::SubtitleCacheStore* subtitleCache, QObject* parent)
    : QObject(parent)
    , m_http(http)
    , m_tokens(tokens)
    , m_subtitleSettings(subtitleSettings)
    , m_cacheSettings(cacheSettings)
    , m_subtitleCache(subtitleCache)
{
}

QStringList SubtitlesSettingsViewModel::preferredLanguages() const
{
    return m_subtitleSettings.preferredLanguages();
}
QString SubtitlesSettingsViewModel::hearingImpaired() const
{
    return m_subtitleSettings.hearingImpaired();
}
QString SubtitlesSettingsViewModel::foreignPartsOnly() const
{
    return m_subtitleSettings.foreignPartsOnly();
}
int SubtitlesSettingsViewModel::subtitleBudgetMb() const
{
    return m_cacheSettings.subtitleBudgetMb();
}

QVariantList SubtitlesSettingsViewModel::commonLanguages() const
{
    QVariantList out;
    for (const auto& opt : core::language::commonLanguages()) {
        QVariantMap row;
        row.insert(QStringLiteral("code"), opt.code);
        row.insert(QStringLiteral("display"), opt.display);
        out.append(row);
    }
    return out;
}

QString SubtitlesSettingsViewModel::languageDisplayName(
    const QString& code) const
{
    return core::language::displayName(code);
}

void SubtitlesSettingsViewModel::setApiKey(const QString& v)
{
    if (m_apiKey == v) {
        return;
    }
    m_apiKey = v;
    Q_EMIT credentialInputChanged();
}
void SubtitlesSettingsViewModel::setUsername(const QString& v)
{
    if (m_username == v) {
        return;
    }
    m_username = v;
    Q_EMIT credentialInputChanged();
}
void SubtitlesSettingsViewModel::setPassword(const QString& v)
{
    if (m_password == v) {
        return;
    }
    m_password = v;
    Q_EMIT credentialInputChanged();
}

void SubtitlesSettingsViewModel::setPreferredLanguages(
    const QStringList& codes)
{
    QStringList normalised;
    normalised.reserve(codes.size());
    for (const auto& c : codes) {
        const auto lo = c.trimmed().toLower();
        if (lo.size() == 3 && !normalised.contains(lo)) {
            normalised.append(lo);
        }
    }
    if (m_subtitleSettings.preferredLanguages() == normalised) {
        return;
    }
    m_subtitleSettings.setPreferredLanguages(normalised);
    Q_EMIT preferredLanguagesChanged();
}

void SubtitlesSettingsViewModel::setHearingImpaired(const QString& v)
{
    if (m_subtitleSettings.hearingImpaired() == v) {
        return;
    }
    m_subtitleSettings.setHearingImpaired(v);
    Q_EMIT hearingImpairedChanged();
}

void SubtitlesSettingsViewModel::setForeignPartsOnly(const QString& v)
{
    if (m_subtitleSettings.foreignPartsOnly() == v) {
        return;
    }
    m_subtitleSettings.setForeignPartsOnly(v);
    Q_EMIT foreignPartsOnlyChanged();
}

void SubtitlesSettingsViewModel::setSubtitleBudgetMb(int v)
{
    if (m_cacheSettings.subtitleBudgetMb() == v) {
        return;
    }
    m_cacheSettings.setSubtitleBudgetMb(v);
    Q_EMIT subtitleBudgetMbChanged();
}

void SubtitlesSettingsViewModel::addLanguage(const QString& code)
{
    auto next = preferredLanguages();
    if (next.contains(code)) {
        return;
    }
    next.append(code);
    setPreferredLanguages(next);
}

void SubtitlesSettingsViewModel::removeLanguageAt(int index)
{
    auto next = preferredLanguages();
    if (index < 0 || index >= next.size()) {
        return;
    }
    next.removeAt(index);
    setPreferredLanguages(next);
}

void SubtitlesSettingsViewModel::moveLanguage(int from, int to)
{
    auto next = preferredLanguages();
    if (from < 0 || from >= next.size() || to < 0
        || to >= next.size() || from == to) {
        return;
    }
    next.move(from, to);
    setPreferredLanguages(next);
}

void SubtitlesSettingsViewModel::load()
{
    auto t = loadTask();
    Q_UNUSED(t);
}
void SubtitlesSettingsViewModel::testConnection()
{
    auto t = testTask();
    Q_UNUSED(t);
}
void SubtitlesSettingsViewModel::saveCredentials()
{
    auto t = saveTask();
    Q_UNUSED(t);
}
void SubtitlesSettingsViewModel::removeCredentials()
{
    auto t = removeTask();
    Q_UNUSED(t);
}

void SubtitlesSettingsViewModel::clearCache()
{
    if (!m_subtitleCache) {
        return;
    }
    const auto entries = m_subtitleCache->all();
    for (const auto& e : entries) {
        if (!e.localPath.isEmpty()) {
            QFile::remove(e.localPath);
        }
    }
    m_subtitleCache->clearAll();
    setStatus(i18nc("@info subtitle cache cleared",
                  "Subtitle cache cleared (%1 file(s)).",
                  entries.size()),
        kStatusPositive);
}

void SubtitlesSettingsViewModel::setStatus(const QString& message, int kind)
{
    if (m_statusMessage == message && m_statusKind == kind) {
        return;
    }
    m_statusMessage = message;
    m_statusKind = kind;
    Q_EMIT statusChanged();
}

void SubtitlesSettingsViewModel::setBusy(bool on)
{
    if (m_busy == on) {
        return;
    }
    m_busy = on;
    Q_EMIT busyChanged();
}

QCoro::Task<void> SubtitlesSettingsViewModel::loadTask()
{
    setBusy(true);
    try {
        const auto apiKey = co_await m_tokens->read(
            QString::fromLatin1(core::TokenStore::kOpenSubtitlesApiKey));
        const auto username = co_await m_tokens->read(
            QString::fromLatin1(core::TokenStore::kOpenSubtitlesUsername));
        const auto password = co_await m_tokens->read(
            QString::fromLatin1(core::TokenStore::kOpenSubtitlesPassword));
        if (!apiKey.isEmpty()) {
            m_apiKey = apiKey;
        }
        if (!username.isEmpty()) {
            m_username = username;
        }
        if (!password.isEmpty()) {
            m_password = password;
        }
        Q_EMIT credentialInputChanged();
        const bool saved = !apiKey.isEmpty() && !username.isEmpty()
            && !password.isEmpty();
        if (saved != m_credentialsSaved) {
            m_credentialsSaved = saved;
            Q_EMIT credentialsSavedChanged();
        }
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "subtitles settings/load"),
            kStatusError);
    }
    setBusy(false);
}

QCoro::Task<void> SubtitlesSettingsViewModel::testTask()
{
    const auto apiKey = m_apiKey.trimmed();
    const auto username = m_username.trimmed();
    const auto password = m_password;
    if (apiKey.isEmpty() || username.isEmpty() || password.isEmpty()) {
        co_return;
    }
    setBusy(true);
    setStatus(i18nc("@info subtitles settings status, in progress",
        "Testing OpenSubtitles credentials…"), kStatusInfo);
    api::OpenSubtitlesClient client(m_http, apiKey, username,
        password);
    try {
        co_await client.ensureLoggedIn();
        api::SubtitleSearchQuery q;
        q.key.kind = api::MediaKind::Movie;
        q.key.imdbId = QStringLiteral("tt0133093");
        const auto hits = co_await client.search(q);
        setStatus(i18nc("@info subtitles connection probe",
            "Connected · %1 results found.", hits.size()),
            kStatusPositive);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "subtitles settings/test"),
            kStatusError);
    }
    setBusy(false);
}

QCoro::Task<void> SubtitlesSettingsViewModel::saveTask()
{
    const auto apiKey = m_apiKey.trimmed();
    const auto username = m_username.trimmed();
    const auto password = m_password;
    if (apiKey.isEmpty() || username.isEmpty() || password.isEmpty()) {
        co_return;
    }
    setBusy(true);
    try {
        co_await m_tokens->write(
            QString::fromLatin1(core::TokenStore::kOpenSubtitlesApiKey),
            apiKey);
        co_await m_tokens->write(
            QString::fromLatin1(core::TokenStore::kOpenSubtitlesUsername),
            username);
        co_await m_tokens->write(
            QString::fromLatin1(core::TokenStore::kOpenSubtitlesPassword),
            password);
        if (!m_credentialsSaved) {
            m_credentialsSaved = true;
            Q_EMIT credentialsSavedChanged();
        }
        Q_EMIT credentialsChanged();
        setStatus(i18nc("@info subtitles settings status",
            "Credentials saved to keyring."), kStatusPositive);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "subtitles settings/save"),
            kStatusError);
    }
    setBusy(false);
}

QCoro::Task<void> SubtitlesSettingsViewModel::removeTask()
{
    setBusy(true);
    try {
        co_await m_tokens->remove(
            QString::fromLatin1(core::TokenStore::kOpenSubtitlesApiKey));
        co_await m_tokens->remove(
            QString::fromLatin1(core::TokenStore::kOpenSubtitlesUsername));
        co_await m_tokens->remove(
            QString::fromLatin1(core::TokenStore::kOpenSubtitlesPassword));
        m_apiKey.clear();
        m_username.clear();
        m_password.clear();
        Q_EMIT credentialInputChanged();
        if (m_credentialsSaved) {
            m_credentialsSaved = false;
            Q_EMIT credentialsSavedChanged();
        }
        Q_EMIT credentialsChanged();
        setStatus(i18nc("@info subtitles settings status",
            "Credentials removed from keyring."), kStatusInfo);
    } catch (const core::TokenStoreError& e) {
        setStatus(e.message(), kStatusError);
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "subtitles settings/remove"),
            kStatusError);
    }
    setBusy(false);
}

// ========================== Torrent streaming =============================

TorrentStreamingSettingsViewModel::TorrentStreamingSettingsViewModel(
    config::TorrentStreamingSettings& settings,
    core::MediaCache& cache, QObject* parent)
    : QObject(parent)
    , m_settings(settings)
    , m_cache(cache)
{
    // Cache stats are read on demand from `MediaCache` (which walks
    // the asset dirs). A 5 s poll keeps the settings page's usage
    // bar live without making per-property reads expensive.
    auto* poll = new QTimer(this);
    poll->setInterval(5000);
    poll->setTimerType(Qt::CoarseTimer);
    connect(poll, &QTimer::timeout, this,
        [this] { Q_EMIT cacheChanged(); });
    poll->start();
}

int TorrentStreamingSettingsViewModel::cacheBudgetGb() const { return m_settings.cacheBudgetGb(); }
int TorrentStreamingSettingsViewModel::startupBufferMiB() const { return m_settings.startupBufferMiB(); }
int TorrentStreamingSettingsViewModel::readaheadMiB() const { return m_settings.readaheadMiB(); }
int TorrentStreamingSettingsViewModel::tailBufferMiB() const { return m_settings.tailBufferMiB(); }
int TorrentStreamingSettingsViewModel::maxDownloadRateKiB() const { return m_settings.maxDownloadRateKiB(); }
int TorrentStreamingSettingsViewModel::maxUploadRateKiB() const { return m_settings.maxUploadRateKiB(); }
int TorrentStreamingSettingsViewModel::idleStopMinutes() const { return m_settings.idleStopMinutes(); }

void TorrentStreamingSettingsViewModel::setCacheBudgetGb(int v)
{
    if (cacheBudgetGb() == v) return;
    m_settings.setCacheBudgetGb(v);
    Q_EMIT cacheBudgetGbChanged();
    // Budget changed → the usage bar's denominator changed too.
    Q_EMIT cacheChanged();
}
void TorrentStreamingSettingsViewModel::setStartupBufferMiB(int v)
{
    if (startupBufferMiB() == v) return;
    m_settings.setStartupBufferMiB(v);
    Q_EMIT startupBufferMiBChanged();
}
void TorrentStreamingSettingsViewModel::setReadaheadMiB(int v)
{
    if (readaheadMiB() == v) return;
    m_settings.setReadaheadMiB(v);
    Q_EMIT readaheadMiBChanged();
}
void TorrentStreamingSettingsViewModel::setTailBufferMiB(int v)
{
    if (tailBufferMiB() == v) return;
    m_settings.setTailBufferMiB(v);
    Q_EMIT tailBufferMiBChanged();
}
void TorrentStreamingSettingsViewModel::setMaxDownloadRateKiB(int v)
{
    if (maxDownloadRateKiB() == v) return;
    m_settings.setMaxDownloadRateKiB(v);
    Q_EMIT maxDownloadRateKiBChanged();
}
void TorrentStreamingSettingsViewModel::setMaxUploadRateKiB(int v)
{
    if (maxUploadRateKiB() == v) return;
    m_settings.setMaxUploadRateKiB(v);
    Q_EMIT maxUploadRateKiBChanged();
}
void TorrentStreamingSettingsViewModel::setIdleStopMinutes(int v)
{
    if (idleStopMinutes() == v) return;
    m_settings.setIdleStopMinutes(v);
    Q_EMIT idleStopMinutesChanged();
}

qint64 TorrentStreamingSettingsViewModel::cacheSizeBytes() const
{
    return m_cache.sizeBytes();
}

qint64 TorrentStreamingSettingsViewModel::cacheBudgetBytes() const
{
    return m_cache.budgetBytes();
}

qint64 TorrentStreamingSettingsViewModel::ephemeralSizeBytes() const
{
    return m_cache.ephemeralSizeBytes();
}

double TorrentStreamingSettingsViewModel::cacheUsageFraction() const
{
    const qint64 budget = cacheBudgetBytes();
    if (budget <= 0) {
        return 0.0;
    }
    return qBound(0.0,
        static_cast<double>(cacheSizeBytes())
            / static_cast<double>(budget),
        1.0);
}

QString TorrentStreamingSettingsViewModel::cacheSizeText() const
{
    return KFormat().formatByteSize(cacheSizeBytes());
}

QString TorrentStreamingSettingsViewModel::cacheBudgetText() const
{
    return KFormat().formatByteSize(cacheBudgetBytes());
}

QString TorrentStreamingSettingsViewModel::ephemeralSizeText() const
{
    return KFormat().formatByteSize(ephemeralSizeBytes());
}

void TorrentStreamingSettingsViewModel::runEvictionNow()
{
    qCInfo(KINEMA_UI)
        << "Downloads settings: manual cache eviction triggered";
    m_cache.enforceBudget();
    Q_EMIT cacheChanged();
}

// ============================== Root ======================================

SettingsRootViewModel::SettingsRootViewModel(core::HttpClient* http,
    core::TokenStore* tokens, api::IndexerSelector* indexers,
    config::AppSettings& settings,
    core::SubtitleCacheStore* subtitleCache,
    core::MediaCache* mediaCache, QObject* parent)
    : QObject(parent)
{
    Q_ASSERT(mediaCache);
    m_general = new GeneralSettingsViewModel(settings.search(),
        settings.appearance(), this);
    m_tmdb = new TmdbSettingsViewModel(http, tokens, this);
    m_debrid = new DebridSettingsViewModel(http, tokens,
        settings.debrid(), this);
    m_indexers = new IndexerSettingsViewModel(indexers,
        settings.indexers(), settings.torrentio(),
        settings.peerflix(), this);
    m_streams = new StreamsSettingsViewModel(settings.filter(), this);
    m_player = new PlayerSettingsViewModel(settings.player(), this);
    m_subs = new SubtitlesSettingsViewModel(http, tokens,
        settings.subtitle(), settings.cache(), subtitleCache, this);
    m_torrentStreaming = new TorrentStreamingSettingsViewModel(
        settings.torrentStreaming(), *mediaCache, this);

    // Forward token / credential changes through the root so
    // `MainController` can route them to `TokenController`.
    connect(m_tmdb, &TmdbSettingsViewModel::tokenChanged, this,
        &SettingsRootViewModel::tmdbTokenChanged);
    connect(m_debrid->realDebrid(),
        &RealDebridSectionViewModel::tokenChanged, this,
        &SettingsRootViewModel::realDebridTokenChanged);
    connect(m_debrid->allDebrid(),
        &AllDebridSectionViewModel::apiKeyChanged, this,
        &SettingsRootViewModel::allDebridApiKeyChanged);
    connect(m_debrid,
        &DebridSettingsViewModel::activeProviderChanged, this,
        &SettingsRootViewModel::activeDebridProviderChanged);
    connect(m_subs,
        &SubtitlesSettingsViewModel::credentialsChanged, this,
        &SettingsRootViewModel::subtitleCredentialsChanged);

    // Initial async loads. Each VM's loadTask is independent and
    // safe to run concurrently — they hit different keyring keys.
    m_tmdb->load();
    m_debrid->realDebrid()->load();
    m_debrid->allDebrid()->load();
    m_subs->load();
}

} // namespace kinema::ui::qml
