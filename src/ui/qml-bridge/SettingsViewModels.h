// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>
#include <QString>
#include <QStringList>
#include <QStringListModel>
#include <QVariantList>


#include <QCoro/QCoroTask>

namespace kinema::core {
class HttpClient;
class SubtitleCacheStore;
class TokenStore;
}

namespace kinema::config {
class AppearanceSettings;
class AppSettings;
class CacheSettings;
class FilterSettings;
class PlayerSettings;
class RealDebridSettings;
class SearchSettings;
class SubtitleSettings;
class TorrentioSettings;
}

namespace kinema::ui::qml {

/**
 * Per-page settings view-models used by the Kirigami settings tree.
 *
 * Every VM:
 *   - Holds a reference to one or more `config::*Settings` objects.
 *   - Exposes their fields as `Q_PROPERTY` with WRITE setters that
 *     persist immediately. There is no Apply/OK/Cancel; writes hit
 *     KConfig the moment the user toggles a switch.
 *   - For credentials (TMDB / Real-Debrid / OpenSubtitles) keeps an
 *     async test/save/remove pattern so token validation stays
 *     transactional.
 *
 * VMs are owned by `SettingsRootViewModel`, which `MainController`
 * exposes to QML as the `settingsVm` context property.
 */

// ---- General ----------------------------------------------------------
class GeneralSettingsViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(int defaultSearchKind READ defaultSearchKind
        WRITE setDefaultSearchKind NOTIFY defaultSearchKindChanged)
    Q_PROPERTY(int defaultTorrentioSort READ defaultTorrentioSort
        WRITE setDefaultTorrentioSort NOTIFY defaultTorrentioSortChanged)
    Q_PROPERTY(bool closeToTray READ closeToTray
        WRITE setCloseToTray NOTIFY closeToTrayChanged)
    Q_PROPERTY(bool trayAvailable READ trayAvailable CONSTANT)

public:
    GeneralSettingsViewModel(config::SearchSettings& search,
        config::TorrentioSettings& torrentio,
        config::AppearanceSettings& appearance,
        QObject* parent = nullptr);

    int defaultSearchKind() const;
    int defaultTorrentioSort() const;
    bool closeToTray() const;
    bool trayAvailable() const;

    void setDefaultSearchKind(int kind);
    void setDefaultTorrentioSort(int sort);
    void setCloseToTray(bool on);

Q_SIGNALS:
    void defaultSearchKindChanged();
    void defaultTorrentioSortChanged();
    void closeToTrayChanged();

private:
    config::SearchSettings& m_search;
    config::TorrentioSettings& m_torrentio;
    config::AppearanceSettings& m_appearance;
};

// ---- TMDB -------------------------------------------------------------
class TmdbSettingsViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString token READ token WRITE setToken NOTIFY tokenInputChanged)
    Q_PROPERTY(bool userTokenSaved READ userTokenSaved NOTIFY userTokenSavedChanged)
    Q_PROPERTY(bool hasCompiledDefault READ hasCompiledDefault CONSTANT)
    Q_PROPERTY(QString statusMessage READ statusMessage NOTIFY statusChanged)
    /// 0 = info, 1 = positive, 2 = error. Maps to
    /// `Kirigami.MessageType` in QML.
    Q_PROPERTY(int statusKind READ statusKind NOTIFY statusChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)

public:
    TmdbSettingsViewModel(core::HttpClient* http,
        core::TokenStore* tokens, QObject* parent = nullptr);

    QString token() const { return m_token; }
    bool userTokenSaved() const { return m_userTokenSaved; }
    bool hasCompiledDefault() const;
    QString statusMessage() const { return m_statusMessage; }
    int statusKind() const { return m_statusKind; }
    bool busy() const { return m_busy; }

    void setToken(const QString& token);

public Q_SLOTS:
    void load();
    void testConnection();
    void save();
    void remove();

Q_SIGNALS:
    void tokenInputChanged();
    void userTokenSavedChanged();
    void statusChanged();
    void busyChanged();
    void tokenChanged(const QString& token);

private:
    void setStatus(const QString& message, int kind);
    void setBusy(bool on);
    QCoro::Task<void> loadTask();
    QCoro::Task<void> testTask();
    QCoro::Task<void> saveTask();
    QCoro::Task<void> removeTask();

    core::HttpClient* m_http;
    core::TokenStore* m_tokens;
    QString m_token;
    bool m_userTokenSaved = false;
    QString m_statusMessage;
    int m_statusKind = 0;
    bool m_busy = false;
};

// ---- Real-Debrid ------------------------------------------------------
class RealDebridSettingsViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString token READ token WRITE setToken NOTIFY tokenInputChanged)
    Q_PROPERTY(bool tokenSaved READ tokenSaved NOTIFY tokenSavedChanged)
    Q_PROPERTY(QString statusMessage READ statusMessage NOTIFY statusChanged)
    Q_PROPERTY(int statusKind READ statusKind NOTIFY statusChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)

public:
    RealDebridSettingsViewModel(core::HttpClient* http,
        core::TokenStore* tokens,
        config::RealDebridSettings& settings,
        QObject* parent = nullptr);

    QString token() const { return m_token; }
    bool tokenSaved() const;
    QString statusMessage() const { return m_statusMessage; }
    int statusKind() const { return m_statusKind; }
    bool busy() const { return m_busy; }

    void setToken(const QString& token);

public Q_SLOTS:
    void load();
    void testConnection();
    void save();
    void remove();

Q_SIGNALS:
    void tokenInputChanged();
    void tokenSavedChanged();
    void statusChanged();
    void busyChanged();
    void tokenChanged(const QString& token);

private:
    void setStatus(const QString& message, int kind);
    void setBusy(bool on);
    QCoro::Task<void> loadTask();
    QCoro::Task<void> testTask();
    QCoro::Task<void> saveTask();
    QCoro::Task<void> removeTask();

    core::HttpClient* m_http;
    core::TokenStore* m_tokens;
    config::RealDebridSettings& m_rdSettings;
    QString m_token;
    QString m_statusMessage;
    int m_statusKind = 0;
    bool m_busy = false;
};

// ---- Filters ----------------------------------------------------------
class FiltersSettingsViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QStringList excludedResolutions READ excludedResolutions
        WRITE setExcludedResolutions NOTIFY excludedResolutionsChanged)
    Q_PROPERTY(QStringList excludedCategories READ excludedCategories
        WRITE setExcludedCategories NOTIFY excludedCategoriesChanged)
    Q_PROPERTY(QString blocklistText READ blocklistText
        WRITE setBlocklistText NOTIFY blocklistChanged)
    Q_PROPERTY(QVariantList resolutionOptions READ resolutionOptions CONSTANT)
    Q_PROPERTY(QVariantList categoryOptions READ categoryOptions CONSTANT)

public:
    FiltersSettingsViewModel(config::FilterSettings& settings,
        QObject* parent = nullptr);

    QStringList excludedResolutions() const;
    QStringList excludedCategories() const;
    QString blocklistText() const;
    QVariantList resolutionOptions() const;
    QVariantList categoryOptions() const;

    void setExcludedResolutions(const QStringList& tokens);
    void setExcludedCategories(const QStringList& tokens);
    void setBlocklistText(const QString& text);

    /// Toggle helpers for QML check delegates.
    Q_INVOKABLE void toggleResolution(const QString& token, bool on);
    Q_INVOKABLE void toggleCategory(const QString& token, bool on);
    Q_INVOKABLE bool resolutionExcluded(const QString& token) const;
    Q_INVOKABLE bool categoryExcluded(const QString& token) const;

Q_SIGNALS:
    void excludedResolutionsChanged();
    void excludedCategoriesChanged();
    void blocklistChanged();

private:
    config::FilterSettings& m_settings;
};

// ---- Player -----------------------------------------------------------
class PlayerSettingsViewModel : public QObject
{
    Q_OBJECT
    /// 0 = Embedded, 1 = Mpv, 2 = Vlc, 3 = Custom.
    Q_PROPERTY(int preferredPlayer READ preferredPlayer
        WRITE setPreferredPlayer NOTIFY preferredPlayerChanged)
    Q_PROPERTY(QString customCommand READ customCommand
        WRITE setCustomCommand NOTIFY customCommandChanged)
    Q_PROPERTY(bool embeddedAvailable READ embeddedAvailable CONSTANT)
    Q_PROPERTY(bool mpvAvailable READ mpvAvailable CONSTANT)
    Q_PROPERTY(bool vlcAvailable READ vlcAvailable CONSTANT)

    Q_PROPERTY(bool hardwareDecoding READ hardwareDecoding
        WRITE setHardwareDecoding NOTIFY hardwareDecodingChanged)
    Q_PROPERTY(QString preferredAudioLang READ preferredAudioLang
        WRITE setPreferredAudioLang NOTIFY preferredAudioLangChanged)
    Q_PROPERTY(QString preferredSubtitleLang READ preferredSubtitleLang
        WRITE setPreferredSubtitleLang NOTIFY preferredSubtitleLangChanged)

    Q_PROPERTY(bool autoplayNextEpisode READ autoplayNextEpisode
        WRITE setAutoplayNextEpisode NOTIFY autoplayNextEpisodeChanged)
    Q_PROPERTY(bool skipIntroChapters READ skipIntroChapters
        WRITE setSkipIntroChapters NOTIFY skipIntroChaptersChanged)
    Q_PROPERTY(int resumePromptThresholdSec READ resumePromptThresholdSec
        WRITE setResumePromptThresholdSec NOTIFY resumePromptThresholdSecChanged)
    Q_PROPERTY(int autoNextCountdownSec READ autoNextCountdownSec
        WRITE setAutoNextCountdownSec NOTIFY autoNextCountdownSecChanged)

public:
    PlayerSettingsViewModel(config::PlayerSettings& settings,
        QObject* parent = nullptr);

    int preferredPlayer() const;
    QString customCommand() const;
    bool embeddedAvailable() const;
    bool mpvAvailable() const;
    bool vlcAvailable() const;
    bool hardwareDecoding() const;
    QString preferredAudioLang() const;
    QString preferredSubtitleLang() const;
    bool autoplayNextEpisode() const;
    bool skipIntroChapters() const;
    int resumePromptThresholdSec() const;
    int autoNextCountdownSec() const;

    void setPreferredPlayer(int kind);
    void setCustomCommand(const QString& cmd);
    void setHardwareDecoding(bool on);
    void setPreferredAudioLang(const QString& v);
    void setPreferredSubtitleLang(const QString& v);
    void setAutoplayNextEpisode(bool on);
    void setSkipIntroChapters(bool on);
    void setResumePromptThresholdSec(int v);
    void setAutoNextCountdownSec(int v);

Q_SIGNALS:
    void preferredPlayerChanged();
    void customCommandChanged();
    void hardwareDecodingChanged();
    void preferredAudioLangChanged();
    void preferredSubtitleLangChanged();
    void autoplayNextEpisodeChanged();
    void skipIntroChaptersChanged();
    void resumePromptThresholdSecChanged();
    void autoNextCountdownSecChanged();

private:
    config::PlayerSettings& m_settings;
};

// ---- Subtitles --------------------------------------------------------
class SubtitlesSettingsViewModel : public QObject
{
    Q_OBJECT

    Q_PROPERTY(QString apiKey READ apiKey WRITE setApiKey NOTIFY credentialInputChanged)
    Q_PROPERTY(QString username READ username WRITE setUsername NOTIFY credentialInputChanged)
    Q_PROPERTY(QString password READ password WRITE setPassword NOTIFY credentialInputChanged)
    Q_PROPERTY(bool credentialsSaved READ credentialsSaved NOTIFY credentialsSavedChanged)
    Q_PROPERTY(QString statusMessage READ statusMessage NOTIFY statusChanged)
    Q_PROPERTY(int statusKind READ statusKind NOTIFY statusChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)

    Q_PROPERTY(QStringList preferredLanguages READ preferredLanguages
        WRITE setPreferredLanguages NOTIFY preferredLanguagesChanged)
    Q_PROPERTY(QString hearingImpaired READ hearingImpaired
        WRITE setHearingImpaired NOTIFY hearingImpairedChanged)
    Q_PROPERTY(QString foreignPartsOnly READ foreignPartsOnly
        WRITE setForeignPartsOnly NOTIFY foreignPartsOnlyChanged)
    Q_PROPERTY(int subtitleBudgetMb READ subtitleBudgetMb
        WRITE setSubtitleBudgetMb NOTIFY subtitleBudgetMbChanged)

    Q_PROPERTY(QVariantList commonLanguages READ commonLanguages CONSTANT)

public:
    SubtitlesSettingsViewModel(core::HttpClient* http,
        core::TokenStore* tokens,
        config::SubtitleSettings& subtitleSettings,
        config::CacheSettings& cacheSettings,
        core::SubtitleCacheStore* subtitleCache,
        QObject* parent = nullptr);

    QString apiKey() const { return m_apiKey; }
    QString username() const { return m_username; }
    QString password() const { return m_password; }
    bool credentialsSaved() const { return m_credentialsSaved; }
    QString statusMessage() const { return m_statusMessage; }
    int statusKind() const { return m_statusKind; }
    bool busy() const { return m_busy; }

    QStringList preferredLanguages() const;
    QString hearingImpaired() const;
    QString foreignPartsOnly() const;
    int subtitleBudgetMb() const;
    QVariantList commonLanguages() const;

    void setApiKey(const QString& v);
    void setUsername(const QString& v);
    void setPassword(const QString& v);
    void setPreferredLanguages(const QStringList& codes);
    void setHearingImpaired(const QString& v);
    void setForeignPartsOnly(const QString& v);
    void setSubtitleBudgetMb(int v);

    Q_INVOKABLE QString languageDisplayName(const QString& code) const;
    Q_INVOKABLE void addLanguage(const QString& code);
    Q_INVOKABLE void removeLanguageAt(int index);
    Q_INVOKABLE void moveLanguage(int from, int to);

public Q_SLOTS:
    void load();
    void testConnection();
    void saveCredentials();
    void removeCredentials();
    void clearCache();

Q_SIGNALS:
    void credentialInputChanged();
    void credentialsSavedChanged();
    void statusChanged();
    void busyChanged();
    void preferredLanguagesChanged();
    void hearingImpairedChanged();
    void foreignPartsOnlyChanged();
    void subtitleBudgetMbChanged();
    void credentialsChanged();

private:
    void setStatus(const QString& message, int kind);
    void setBusy(bool on);
    QCoro::Task<void> loadTask();
    QCoro::Task<void> testTask();
    QCoro::Task<void> saveTask();
    QCoro::Task<void> removeTask();

    core::HttpClient* m_http;
    core::TokenStore* m_tokens;
    config::SubtitleSettings& m_subtitleSettings;
    config::CacheSettings& m_cacheSettings;
    core::SubtitleCacheStore* m_subtitleCache;
    QString m_apiKey;
    QString m_username;
    QString m_password;
    bool m_credentialsSaved = false;
    QString m_statusMessage;
    int m_statusKind = 0;
    bool m_busy = false;
};

// ---- Appearance -------------------------------------------------------
// Survives as a sub-page even though only `closeToTray` and (until
// phase 02) `showMenuBar` were ever surfaced. The remaining splitter
// state keys are storage-only — they don't appear in the UI.
class AppearanceSettingsViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(bool closeToTray READ closeToTray WRITE setCloseToTray NOTIFY closeToTrayChanged)
    Q_PROPERTY(bool trayAvailable READ trayAvailable CONSTANT)

public:
    AppearanceSettingsViewModel(
        config::AppearanceSettings& appearance,
        QObject* parent = nullptr);

    bool closeToTray() const;
    bool trayAvailable() const;
    void setCloseToTray(bool on);

Q_SIGNALS:
    void closeToTrayChanged();

private:
    config::AppearanceSettings& m_appearance;
};

// ---- Root -------------------------------------------------------------
class SettingsRootViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(GeneralSettingsViewModel* general READ general CONSTANT)
    Q_PROPERTY(TmdbSettingsViewModel* tmdb READ tmdb CONSTANT)
    Q_PROPERTY(RealDebridSettingsViewModel* realDebrid READ realDebrid CONSTANT)
    Q_PROPERTY(FiltersSettingsViewModel* filters READ filters CONSTANT)
    Q_PROPERTY(PlayerSettingsViewModel* player READ player CONSTANT)
    Q_PROPERTY(SubtitlesSettingsViewModel* subtitles READ subtitles CONSTANT)
    Q_PROPERTY(AppearanceSettingsViewModel* appearance READ appearance CONSTANT)
    /// Optional sub-page key the shell should select on push.
    /// Cleared after consumption so re-pushes default again.
    Q_PROPERTY(QString initialCategory READ initialCategory NOTIFY initialCategoryChanged)

public:
    SettingsRootViewModel(core::HttpClient* http,
        core::TokenStore* tokens,
        config::AppSettings& settings,
        core::SubtitleCacheStore* subtitleCache,
        QObject* parent = nullptr);

    GeneralSettingsViewModel* general() const { return m_general; }
    TmdbSettingsViewModel* tmdb() const { return m_tmdb; }
    RealDebridSettingsViewModel* realDebrid() const { return m_rd; }
    FiltersSettingsViewModel* filters() const { return m_filters; }
    PlayerSettingsViewModel* player() const { return m_player; }
    SubtitlesSettingsViewModel* subtitles() const { return m_subs; }
    AppearanceSettingsViewModel* appearance() const { return m_appear; }
    QString initialCategory() const { return m_initialCategory; }

    void setInitialCategory(const QString& key);
    Q_INVOKABLE void clearInitialCategory();

Q_SIGNALS:
    void initialCategoryChanged();
    /// Forwarded from `TmdbSettingsViewModel::tokenChanged`.
    void tmdbTokenChanged(const QString& token);
    /// Forwarded from `RealDebridSettingsViewModel::tokenChanged`.
    void realDebridTokenChanged(const QString& token);
    /// Forwarded from `SubtitlesSettingsViewModel::credentialsChanged`.
    void subtitleCredentialsChanged();

private:
    GeneralSettingsViewModel* m_general {};
    TmdbSettingsViewModel* m_tmdb {};
    RealDebridSettingsViewModel* m_rd {};
    FiltersSettingsViewModel* m_filters {};
    PlayerSettingsViewModel* m_player {};
    SubtitlesSettingsViewModel* m_subs {};
    AppearanceSettingsViewModel* m_appear {};
    QString m_initialCategory;
};

} // namespace kinema::ui::qml
