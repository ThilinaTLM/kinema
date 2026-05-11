// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>
#include <QString>
#include <QStringList>
#include <QStringListModel>
#include <QVariantList>


#include <QCoro/QCoroTask>

namespace kinema::api {
class IndexerSelector;
}

namespace kinema::core {
class HttpClient;
class MediaCache;
class SubtitleCacheStore;
class TokenStore;
}

namespace kinema::config {
class AppearanceSettings;
class AppSettings;
class CacheSettings;
class DebridSettings;
class FilterSettings;
class PlayerSettings;
class SearchSettings;
class IndexerSettings;
class PeerflixSettings;
class SubtitleSettings;
class TorrentioSettings;
class TorrentStreamingSettings;
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

// ---- Application ------------------------------------------------------
class GeneralSettingsViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(int defaultSearchKind READ defaultSearchKind
        WRITE setDefaultSearchKind NOTIFY defaultSearchKindChanged)
    Q_PROPERTY(bool closeToTray READ closeToTray
        WRITE setCloseToTray NOTIFY closeToTrayChanged)
    Q_PROPERTY(bool trayAvailable READ trayAvailable CONSTANT)

public:
    GeneralSettingsViewModel(config::SearchSettings& search,
        config::AppearanceSettings& appearance,
        QObject* parent = nullptr);

    int defaultSearchKind() const;
    bool closeToTray() const;
    bool trayAvailable() const;

    void setDefaultSearchKind(int kind);
    void setCloseToTray(bool on);

Q_SIGNALS:
    void defaultSearchKindChanged();
    void closeToTrayChanged();

private:
    config::SearchSettings& m_search;
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

// ---- Debrid: Real-Debrid section --------------------------------------
class RealDebridSectionViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString token READ token WRITE setToken NOTIFY tokenInputChanged)
    Q_PROPERTY(bool tokenSaved READ tokenSaved NOTIFY tokenSavedChanged)
    Q_PROPERTY(QString statusMessage READ statusMessage NOTIFY statusChanged)
    Q_PROPERTY(int statusKind READ statusKind NOTIFY statusChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)

public:
    RealDebridSectionViewModel(core::HttpClient* http,
        core::TokenStore* tokens,
        config::DebridSettings& settings,
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
    config::DebridSettings& m_settings;
    QString m_token;
    QString m_statusMessage;
    int m_statusKind = 0;
    bool m_busy = false;
};

// ---- Debrid: AllDebrid section ---------------------------------------
class AllDebridSectionViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString apiKey READ apiKey WRITE setApiKey NOTIFY apiKeyInputChanged)
    Q_PROPERTY(bool apiKeySaved READ apiKeySaved NOTIFY apiKeySavedChanged)
    Q_PROPERTY(QString statusMessage READ statusMessage NOTIFY statusChanged)
    Q_PROPERTY(int statusKind READ statusKind NOTIFY statusChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)

public:
    AllDebridSectionViewModel(core::HttpClient* http,
        core::TokenStore* tokens,
        config::DebridSettings& settings,
        QObject* parent = nullptr);

    QString apiKey() const { return m_apiKey; }
    bool apiKeySaved() const;
    QString statusMessage() const { return m_statusMessage; }
    int statusKind() const { return m_statusKind; }
    bool busy() const { return m_busy; }

    void setApiKey(const QString& apiKey);

public Q_SLOTS:
    void load();
    void testConnection();
    void save();
    void remove();

Q_SIGNALS:
    void apiKeyInputChanged();
    void apiKeySavedChanged();
    void statusChanged();
    void busyChanged();
    void apiKeyChanged(const QString& apiKey);

private:
    void setStatus(const QString& message, int kind);
    void setBusy(bool on);
    QCoro::Task<void> loadTask();
    QCoro::Task<void> testTask();
    QCoro::Task<void> saveTask();
    QCoro::Task<void> removeTask();

    core::HttpClient* m_http;
    core::TokenStore* m_tokens;
    config::DebridSettings& m_settings;
    QString m_apiKey;
    QString m_statusMessage;
    int m_statusKind = 0;
    bool m_busy = false;
};

// ---- Debrid: parent VM ------------------------------------------------
class DebridSettingsViewModel : public QObject
{
    Q_OBJECT
    /// 0 = None, 1 = Real-Debrid, 2 = AllDebrid. Maps to
    /// `api::DebridProvider`.
    Q_PROPERTY(int activeProvider READ activeProvider
        WRITE setActiveProvider NOTIFY activeProviderChanged)
    Q_PROPERTY(RealDebridSectionViewModel* realDebrid
        READ realDebrid CONSTANT)
    Q_PROPERTY(AllDebridSectionViewModel* allDebrid
        READ allDebrid CONSTANT)

public:
    DebridSettingsViewModel(core::HttpClient* http,
        core::TokenStore* tokens,
        config::DebridSettings& settings,
        QObject* parent = nullptr);

    int activeProvider() const;
    void setActiveProvider(int provider);

    RealDebridSectionViewModel* realDebrid() const { return m_rd; }
    AllDebridSectionViewModel* allDebrid() const { return m_ad; }

Q_SIGNALS:
    void activeProviderChanged();

private:
    config::DebridSettings& m_settings;
    RealDebridSectionViewModel* m_rd {};
    AllDebridSectionViewModel* m_ad {};
};

// ---- Indexers: Torrentio section --------------------------------------
class TorrentioSectionViewModel : public QObject
{
    Q_OBJECT
    /// 0 = Seeders, 1 = Size, 2 = Quality & Size.
    Q_PROPERTY(int defaultSort READ defaultSort
        WRITE setDefaultSort NOTIFY defaultSortChanged)
    Q_PROPERTY(QString baseUrl READ baseUrl
        WRITE setBaseUrl NOTIFY baseUrlChanged)
    Q_PROPERTY(QString defaultBaseUrl READ defaultBaseUrlString CONSTANT)
    Q_PROPERTY(QString statusMessage READ statusMessage NOTIFY statusChanged)
    Q_PROPERTY(int statusKind READ statusKind NOTIFY statusChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)

public:
    TorrentioSectionViewModel(api::IndexerSelector* indexers,
        config::TorrentioSettings& settings,
        QObject* parent = nullptr);

    int defaultSort() const;
    QString baseUrl() const;
    QString defaultBaseUrlString() const;
    QString statusMessage() const { return m_statusMessage; }
    int statusKind() const { return m_statusKind; }
    bool busy() const { return m_busy; }

    void setDefaultSort(int sort);
    void setBaseUrl(const QString& url);

public Q_SLOTS:
    void testConnection();
    void resetBaseUrl();

Q_SIGNALS:
    void defaultSortChanged();
    void baseUrlChanged();
    void statusChanged();
    void busyChanged();

private:
    void setStatus(const QString& message, int kind);
    void setBusy(bool on);
    QCoro::Task<void> testTask();

    api::IndexerSelector* m_indexers;
    config::TorrentioSettings& m_settings;
    QString m_statusMessage;
    int m_statusKind = 0;
    bool m_busy = false;
};

// ---- Indexers: Peerflix section ---------------------------------------
class PeerflixSectionViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString baseUrl READ baseUrl
        WRITE setBaseUrl NOTIFY baseUrlChanged)
    Q_PROPERTY(QString defaultBaseUrl READ defaultBaseUrlString CONSTANT)
    Q_PROPERTY(QString statusMessage READ statusMessage NOTIFY statusChanged)
    Q_PROPERTY(int statusKind READ statusKind NOTIFY statusChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)

public:
    PeerflixSectionViewModel(api::IndexerSelector* indexers,
        config::PeerflixSettings& settings,
        QObject* parent = nullptr);

    QString baseUrl() const;
    QString defaultBaseUrlString() const;
    QString statusMessage() const { return m_statusMessage; }
    int statusKind() const { return m_statusKind; }
    bool busy() const { return m_busy; }

    void setBaseUrl(const QString& url);

public Q_SLOTS:
    void testConnection();
    void resetBaseUrl();

Q_SIGNALS:
    void baseUrlChanged();
    void statusChanged();
    void busyChanged();

private:
    void setStatus(const QString& message, int kind);
    void setBusy(bool on);
    QCoro::Task<void> testTask();

    api::IndexerSelector* m_indexers;
    config::PeerflixSettings& m_settings;
    QString m_statusMessage;
    int m_statusKind = 0;
    bool m_busy = false;
};

// ---- Indexers: parent VM ----------------------------------------------
class IndexerSettingsViewModel : public QObject
{
    Q_OBJECT
    /// 1 = Torrentio, 2 = Peerflix. Maps to `api::IndexerKind`.
    Q_PROPERTY(int activeIndexer READ activeIndexer
        WRITE setActiveIndexer NOTIFY activeIndexerChanged)
    Q_PROPERTY(TorrentioSectionViewModel* torrentio
        READ torrentio CONSTANT)
    Q_PROPERTY(PeerflixSectionViewModel* peerflix
        READ peerflix CONSTANT)

public:
    IndexerSettingsViewModel(api::IndexerSelector* indexers,
        config::IndexerSettings& indexerSettings,
        config::TorrentioSettings& torrentioSettings,
        config::PeerflixSettings& peerflixSettings,
        QObject* parent = nullptr);

    int activeIndexer() const;
    void setActiveIndexer(int kind);

    TorrentioSectionViewModel* torrentio() const { return m_torrentio; }
    PeerflixSectionViewModel* peerflix() const { return m_peerflix; }

Q_SIGNALS:
    void activeIndexerChanged();

private:
    config::IndexerSettings& m_settings;
    TorrentioSectionViewModel* m_torrentio {};
    PeerflixSectionViewModel* m_peerflix {};
};

// ---- Streams (indexer-agnostic filters) -------------------------------
class StreamsSettingsViewModel : public QObject
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
    StreamsSettingsViewModel(config::FilterSettings& settings,
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

    Q_PROPERTY(bool skipIntroChapters READ skipIntroChapters
        WRITE setSkipIntroChapters NOTIFY skipIntroChaptersChanged)
    Q_PROPERTY(int resumePromptThresholdSec READ resumePromptThresholdSec
        WRITE setResumePromptThresholdSec NOTIFY resumePromptThresholdSecChanged)

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
    bool skipIntroChapters() const;
    int resumePromptThresholdSec() const;

    void setPreferredPlayer(int kind);
    void setCustomCommand(const QString& cmd);
    void setHardwareDecoding(bool on);
    void setPreferredAudioLang(const QString& v);
    void setPreferredSubtitleLang(const QString& v);
    void setSkipIntroChapters(bool on);
    void setResumePromptThresholdSec(int v);

Q_SIGNALS:
    void preferredPlayerChanged();
    void customCommandChanged();
    void hardwareDecodingChanged();
    void preferredAudioLangChanged();
    void preferredSubtitleLangChanged();
    void skipIntroChaptersChanged();
    void resumePromptThresholdSecChanged();

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

// ---- Torrent streaming -----------------------------------------------
//
// Despite the historical class name, this VM now also owns the
// downloader's filesystem cache read-outs (`MediaCache::sizeBytes`
// etc.) and the manual eviction trigger. The settings tab is
// surfaced to the user as "Downloads" — the on-disk class /
// KConfig group names are kept stable to avoid a migration on a
// pre-1.0 codebase.
class TorrentStreamingSettingsViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(int cacheBudgetGb READ cacheBudgetGb WRITE setCacheBudgetGb NOTIFY cacheBudgetGbChanged)
    Q_PROPERTY(int startupBufferMiB READ startupBufferMiB WRITE setStartupBufferMiB NOTIFY startupBufferMiBChanged)
    Q_PROPERTY(int readaheadMiB READ readaheadMiB WRITE setReadaheadMiB NOTIFY readaheadMiBChanged)
    Q_PROPERTY(int tailBufferMiB READ tailBufferMiB WRITE setTailBufferMiB NOTIFY tailBufferMiBChanged)
    Q_PROPERTY(int maxDownloadRateKiB READ maxDownloadRateKiB WRITE setMaxDownloadRateKiB NOTIFY maxDownloadRateKiBChanged)
    Q_PROPERTY(int maxUploadRateKiB READ maxUploadRateKiB WRITE setMaxUploadRateKiB NOTIFY maxUploadRateKiBChanged)
    Q_PROPERTY(int idleStopMinutes READ idleStopMinutes WRITE setIdleStopMinutes NOTIFY idleStopMinutesChanged)

    // Live cache state, refreshed by an internal poll timer. Pinned
    // bytes are excluded from the budget; `ephemeralSizeBytes` is
    // what eviction is allowed to drop.
    Q_PROPERTY(qint64 cacheSizeBytes READ cacheSizeBytes NOTIFY cacheChanged)
    Q_PROPERTY(qint64 cacheBudgetBytes READ cacheBudgetBytes NOTIFY cacheChanged)
    Q_PROPERTY(qint64 ephemeralSizeBytes READ ephemeralSizeBytes NOTIFY cacheChanged)
    Q_PROPERTY(double cacheUsageFraction READ cacheUsageFraction NOTIFY cacheChanged)
    Q_PROPERTY(QString cacheSizeText READ cacheSizeText NOTIFY cacheChanged)
    Q_PROPERTY(QString cacheBudgetText READ cacheBudgetText NOTIFY cacheChanged)
    Q_PROPERTY(QString ephemeralSizeText READ ephemeralSizeText NOTIFY cacheChanged)

public:
    TorrentStreamingSettingsViewModel(config::TorrentStreamingSettings& settings,
        core::MediaCache& cache,
        QObject* parent = nullptr);

    int cacheBudgetGb() const;
    int startupBufferMiB() const;
    int readaheadMiB() const;
    int tailBufferMiB() const;
    int maxDownloadRateKiB() const;
    int maxUploadRateKiB() const;
    int idleStopMinutes() const;

    void setCacheBudgetGb(int v);
    void setStartupBufferMiB(int v);
    void setReadaheadMiB(int v);
    void setTailBufferMiB(int v);
    void setMaxDownloadRateKiB(int v);
    void setMaxUploadRateKiB(int v);
    void setIdleStopMinutes(int v);

    qint64 cacheSizeBytes() const;
    qint64 cacheBudgetBytes() const;
    qint64 ephemeralSizeBytes() const;
    double cacheUsageFraction() const;
    QString cacheSizeText() const;
    QString cacheBudgetText() const;
    QString ephemeralSizeText() const;

public Q_SLOTS:
    /// Drop ephemeral cache entries until the budget is satisfied,
    /// then refresh the cached counters.
    void runEvictionNow();

Q_SIGNALS:
    void cacheBudgetGbChanged();
    void startupBufferMiBChanged();
    void readaheadMiBChanged();
    void tailBufferMiBChanged();
    void maxDownloadRateKiBChanged();
    void maxUploadRateKiBChanged();
    void idleStopMinutesChanged();
    void cacheChanged();

private:
    config::TorrentStreamingSettings& m_settings;
    core::MediaCache& m_cache;
};

// ---- Root -------------------------------------------------------------
class SettingsRootViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(GeneralSettingsViewModel* general READ general CONSTANT)
    Q_PROPERTY(TmdbSettingsViewModel* tmdb READ tmdb CONSTANT)
    Q_PROPERTY(DebridSettingsViewModel* debrid READ debrid CONSTANT)
    Q_PROPERTY(IndexerSettingsViewModel* indexers READ indexers CONSTANT)
    Q_PROPERTY(StreamsSettingsViewModel* streams READ streams CONSTANT)
    Q_PROPERTY(PlayerSettingsViewModel* player READ player CONSTANT)
    Q_PROPERTY(SubtitlesSettingsViewModel* subtitles READ subtitles CONSTANT)
    Q_PROPERTY(TorrentStreamingSettingsViewModel* torrentStreaming READ torrentStreaming CONSTANT)

public:
    SettingsRootViewModel(core::HttpClient* http,
        core::TokenStore* tokens,
        api::IndexerSelector* indexers,
        config::AppSettings& settings,
        core::SubtitleCacheStore* subtitleCache,
        core::MediaCache* mediaCache,
        QObject* parent = nullptr);

    GeneralSettingsViewModel* general() const { return m_general; }
    TmdbSettingsViewModel* tmdb() const { return m_tmdb; }
    DebridSettingsViewModel* debrid() const { return m_debrid; }
    IndexerSettingsViewModel* indexers() const { return m_indexers; }
    StreamsSettingsViewModel* streams() const { return m_streams; }
    PlayerSettingsViewModel* player() const { return m_player; }
    SubtitlesSettingsViewModel* subtitles() const { return m_subs; }
    TorrentStreamingSettingsViewModel* torrentStreaming() const { return m_torrentStreaming; }

Q_SIGNALS:
    /// Forwarded from `TmdbSettingsViewModel::tokenChanged`.
    void tmdbTokenChanged(const QString& token);
    /// Forwarded from `RealDebridSectionViewModel::tokenChanged`.
    void realDebridTokenChanged(const QString& token);
    /// Forwarded from `AllDebridSectionViewModel::apiKeyChanged`.
    void allDebridApiKeyChanged(const QString& apiKey);
    /// Forwarded from `DebridSettingsViewModel::activeProviderChanged`.
    void activeDebridProviderChanged();
    /// Forwarded from `SubtitlesSettingsViewModel::credentialsChanged`.
    void subtitleCredentialsChanged();

private:
    GeneralSettingsViewModel* m_general {};
    TmdbSettingsViewModel* m_tmdb {};
    DebridSettingsViewModel* m_debrid {};
    IndexerSettingsViewModel* m_indexers {};
    StreamsSettingsViewModel* m_streams {};
    PlayerSettingsViewModel* m_player {};
    SubtitlesSettingsViewModel* m_subs {};
    TorrentStreamingSettingsViewModel* m_torrentStreaming {};
};

} // namespace kinema::ui::qml
