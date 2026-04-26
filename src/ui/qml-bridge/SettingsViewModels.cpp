// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/SettingsViewModels.h"

#include "api/Media.h"
#include "api/OpenSubtitlesClient.h"
#include "api/RealDebridClient.h"
#include "api/Subtitle.h"
#include "api/TmdbClient.h"
#include "config/AppSettings.h"
#include "config/AppearanceSettings.h"
#include "config/CacheSettings.h"
#include "config/FilterSettings.h"
#include "config/PlayerSettings.h"
#include "config/RealDebridSettings.h"
#include "config/SearchSettings.h"
#include "config/SubtitleSettings.h"
#include "config/TorrentioSettings.h"
#include "core/DateFormat.h"
#include "core/HttpClient.h"
#include "core/HttpError.h"
#include "core/HttpErrorPresenter.h"
#include "core/Language.h"
#include "core/Player.h"
#include "core/SubtitleCacheStore.h"
#include "core/TmdbConfig.h"
#include "core/TokenStore.h"
#include "core/TorrentioConfig.h"
#include "kinema_debug.h"

#include <KLocalizedString>

#include <QFile>
#include <QSystemTrayIcon>

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

// ============================== General ===================================

GeneralSettingsViewModel::GeneralSettingsViewModel(
    config::SearchSettings& search,
    config::TorrentioSettings& torrentio,
    config::AppearanceSettings& appearance, QObject* parent)
    : QObject(parent)
    , m_search(search)
    , m_torrentio(torrentio)
    , m_appearance(appearance)
{
}

int GeneralSettingsViewModel::defaultSearchKind() const
{
    return m_search.kind() == api::MediaKind::Series ? 1 : 0;
}

int GeneralSettingsViewModel::defaultTorrentioSort() const
{
    return sortModeToIndex(m_torrentio.defaultSort());
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

void GeneralSettingsViewModel::setDefaultTorrentioSort(int sort)
{
    const auto next = indexToSortMode(sort);
    if (m_torrentio.defaultSort() == next) {
        return;
    }
    m_torrentio.setDefaultSort(next);
    Q_EMIT defaultTorrentioSortChanged();
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

// ============================== Real-Debrid ===============================

RealDebridSettingsViewModel::RealDebridSettingsViewModel(
    core::HttpClient* http, core::TokenStore* tokens,
    config::RealDebridSettings& settings, QObject* parent)
    : QObject(parent)
    , m_http(http)
    , m_tokens(tokens)
    , m_rdSettings(settings)
{
}

bool RealDebridSettingsViewModel::tokenSaved() const
{
    return m_rdSettings.configured();
}

void RealDebridSettingsViewModel::setToken(const QString& token)
{
    if (m_token == token) {
        return;
    }
    m_token = token;
    Q_EMIT tokenInputChanged();
}

void RealDebridSettingsViewModel::load()
{
    auto t = loadTask();
    Q_UNUSED(t);
}
void RealDebridSettingsViewModel::testConnection()
{
    auto t = testTask();
    Q_UNUSED(t);
}
void RealDebridSettingsViewModel::save()
{
    auto t = saveTask();
    Q_UNUSED(t);
}
void RealDebridSettingsViewModel::remove()
{
    auto t = removeTask();
    Q_UNUSED(t);
}

void RealDebridSettingsViewModel::setStatus(const QString& message, int kind)
{
    if (m_statusMessage == message && m_statusKind == kind) {
        return;
    }
    m_statusMessage = message;
    m_statusKind = kind;
    Q_EMIT statusChanged();
}

void RealDebridSettingsViewModel::setBusy(bool on)
{
    if (m_busy == on) {
        return;
    }
    m_busy = on;
    Q_EMIT busyChanged();
}

QCoro::Task<void> RealDebridSettingsViewModel::loadTask()
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

QCoro::Task<void> RealDebridSettingsViewModel::testTask()
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

QCoro::Task<void> RealDebridSettingsViewModel::saveTask()
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
        m_rdSettings.setConfigured(true);
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

QCoro::Task<void> RealDebridSettingsViewModel::removeTask()
{
    setBusy(true);
    try {
        co_await m_tokens->remove(
            QString::fromLatin1(core::TokenStore::kRealDebridKey));
        m_rdSettings.setConfigured(false);
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

// ============================== Filters ===================================

FiltersSettingsViewModel::FiltersSettingsViewModel(
    config::FilterSettings& settings, QObject* parent)
    : QObject(parent)
    , m_settings(settings)
{
}

QStringList FiltersSettingsViewModel::excludedResolutions() const
{
    return m_settings.excludedResolutions();
}
QStringList FiltersSettingsViewModel::excludedCategories() const
{
    return m_settings.excludedCategories();
}
QString FiltersSettingsViewModel::blocklistText() const
{
    return m_settings.keywordBlocklist().join(QLatin1Char('\n'));
}

QVariantList FiltersSettingsViewModel::resolutionOptions() const
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

QVariantList FiltersSettingsViewModel::categoryOptions() const
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

void FiltersSettingsViewModel::setExcludedResolutions(
    const QStringList& tokens)
{
    if (m_settings.excludedResolutions() == tokens) {
        return;
    }
    m_settings.setExcludedResolutions(tokens);
    Q_EMIT excludedResolutionsChanged();
}

void FiltersSettingsViewModel::setExcludedCategories(
    const QStringList& tokens)
{
    if (m_settings.excludedCategories() == tokens) {
        return;
    }
    m_settings.setExcludedCategories(tokens);
    Q_EMIT excludedCategoriesChanged();
}

void FiltersSettingsViewModel::setBlocklistText(const QString& text)
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

void FiltersSettingsViewModel::toggleResolution(const QString& token,
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

void FiltersSettingsViewModel::toggleCategory(const QString& token,
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

bool FiltersSettingsViewModel::resolutionExcluded(
    const QString& token) const
{
    return excludedResolutions().contains(token);
}

bool FiltersSettingsViewModel::categoryExcluded(
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
bool PlayerSettingsViewModel::autoplayNextEpisode() const
{
    return m_settings.autoplayNextEpisode();
}
bool PlayerSettingsViewModel::skipIntroChapters() const
{
    return m_settings.skipIntroChapters();
}
int PlayerSettingsViewModel::resumePromptThresholdSec() const
{
    return m_settings.resumePromptThresholdSec();
}
int PlayerSettingsViewModel::autoNextCountdownSec() const
{
    return m_settings.autoNextCountdownSec();
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
void PlayerSettingsViewModel::setAutoplayNextEpisode(bool on)
{
    if (m_settings.autoplayNextEpisode() == on) {
        return;
    }
    m_settings.setAutoplayNextEpisode(on);
    Q_EMIT autoplayNextEpisodeChanged();
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
void PlayerSettingsViewModel::setAutoNextCountdownSec(int v)
{
    if (m_settings.autoNextCountdownSec() == v) {
        return;
    }
    m_settings.setAutoNextCountdownSec(v);
    Q_EMIT autoNextCountdownSecChanged();
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

// ============================== Appearance ================================

AppearanceSettingsViewModel::AppearanceSettingsViewModel(
    config::AppearanceSettings& appearance, QObject* parent)
    : QObject(parent)
    , m_appearance(appearance)
{
}

bool AppearanceSettingsViewModel::closeToTray() const
{
    return m_appearance.closeToTray();
}

bool AppearanceSettingsViewModel::trayAvailable() const
{
    return QSystemTrayIcon::isSystemTrayAvailable();
}

void AppearanceSettingsViewModel::setCloseToTray(bool on)
{
    if (m_appearance.closeToTray() == on) {
        return;
    }
    m_appearance.setCloseToTray(on);
    Q_EMIT closeToTrayChanged();
}

// ============================== Root ======================================

SettingsRootViewModel::SettingsRootViewModel(core::HttpClient* http,
    core::TokenStore* tokens, config::AppSettings& settings,
    core::SubtitleCacheStore* subtitleCache, QObject* parent)
    : QObject(parent)
{
    m_general = new GeneralSettingsViewModel(settings.search(),
        settings.torrentio(), settings.appearance(), this);
    m_tmdb = new TmdbSettingsViewModel(http, tokens, this);
    m_rd = new RealDebridSettingsViewModel(http, tokens,
        settings.realDebrid(), this);
    m_filters = new FiltersSettingsViewModel(settings.filter(),
        this);
    m_player = new PlayerSettingsViewModel(settings.player(), this);
    m_subs = new SubtitlesSettingsViewModel(http, tokens,
        settings.subtitle(), settings.cache(), subtitleCache, this);
    m_appear
        = new AppearanceSettingsViewModel(settings.appearance(), this);

    // Forward token / credential changes through the root so
    // `MainController` can route them to `TokenController`.
    connect(m_tmdb, &TmdbSettingsViewModel::tokenChanged, this,
        &SettingsRootViewModel::tmdbTokenChanged);
    connect(m_rd, &RealDebridSettingsViewModel::tokenChanged, this,
        &SettingsRootViewModel::realDebridTokenChanged);
    connect(m_subs,
        &SubtitlesSettingsViewModel::credentialsChanged, this,
        &SettingsRootViewModel::subtitleCredentialsChanged);

    // Initial async loads. Each VM's loadTask is independent and
    // safe to run concurrently — they hit different keyring keys.
    m_tmdb->load();
    m_rd->load();
    m_subs->load();
}

void SettingsRootViewModel::setInitialCategory(const QString& key)
{
    if (m_initialCategory == key) {
        return;
    }
    m_initialCategory = key;
    Q_EMIT initialCategoryChanged();
}

void SettingsRootViewModel::clearInitialCategory()
{
    setInitialCategory(QString {});
}

} // namespace kinema::ui::qml
