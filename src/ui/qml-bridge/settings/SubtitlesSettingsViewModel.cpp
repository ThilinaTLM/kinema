// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/settings/SubtitlesSettingsViewModel.h"
#include "ui/qml-bridge/settings/SettingsStatus.h"
#include "api/OpenSubtitlesClient.h"
#include "config/CacheSettings.h"
#include "config/SubtitleSettings.h"
#include "core/io/HttpClient.h"
#include "core/io/HttpError.h"
#include "core/io/HttpErrorPresenter.h"
#include "core/persistence/SubtitleCacheStore.h"
#include "core/persistence/TokenStore.h"
#include "core/util/Language.h"
#include "domain/Subtitle.h"
#include "kinema_log_ui.h"
#include <KFormat>
#include <KLocalizedString>
#include <QFile>

namespace kinema::ui::qml::settings {

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
        domain::SubtitleSearchQuery q;
        q.key.kind = domain::MediaKind::Movie;
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

} // namespace kinema::ui::qml::settings
