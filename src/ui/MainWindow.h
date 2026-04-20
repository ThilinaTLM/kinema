// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include "api/Types.h"
#include "core/TorrentioConfig.h"

#include <QMainWindow>

#include <QCoro/QCoroTask>

#include <memory>

class QListView;
class QStackedWidget;

namespace kinema::core {
class HttpClient;
class TokenStore;
}

namespace kinema::api {
class CinemetaClient;
class TorrentioClient;
}

namespace kinema::ui {

class DetailPane;
class ImageLoader;
class ResultCardDelegate;
class ResultsModel;
class SearchBar;
class SeriesFocusView;
class StateWidget;

/**
 * Top-level window. Owns the HTTP stack, API clients, image loader, and
 * wires up the search → results → detail → torrents flow as coroutines.
 */
class MainWindow : public QMainWindow
{
    Q_OBJECT
public:
    explicit MainWindow(QWidget* parent = nullptr);
    ~MainWindow() override;

private Q_SLOTS:
    void onQueryRequested(const QString& text, api::MediaKind kind);
    void onResultActivated(const QModelIndex& index);
    void onEpisodeSelected(const api::Episode& ep);
    void onCopyMagnet(const api::Stream& stream);
    void onOpenMagnet(const api::Stream& stream);
    void onCopyDirectUrl(const api::Stream& stream);
    void onOpenDirectUrl(const api::Stream& stream);
    void onBackFromFocus();
    void showAbout();
    void showRealDebridDialog();

private:
    QCoro::Task<void> runSearch(QString text, api::MediaKind kind);
    QCoro::Task<void> loadMovieDetail(api::MetaSummary summary);
    QCoro::Task<void> loadSeriesDetail(api::MetaSummary summary);
    QCoro::Task<void> loadEpisodeStreams(api::Episode episode, QString imdbId);
    QCoro::Task<void> loadRealDebridToken();

    void buildActions();
    void buildLayout();
    core::torrentio::ConfigOptions currentConfig() const;

    // Ownership (parented to this window)
    std::unique_ptr<core::HttpClient> m_http;
    std::unique_ptr<core::TokenStore> m_tokens;
    api::CinemetaClient* m_cinemeta {};
    api::TorrentioClient* m_torrentio {};
    ImageLoader* m_imageLoader {};

    // UI
    SearchBar* m_searchBar {};
    ResultsModel* m_resultsModel {};
    ResultCardDelegate* m_resultsDelegate {};
    QListView* m_resultsView {};
    StateWidget* m_resultsState {};
    QStackedWidget* m_resultsStack {};
    DetailPane* m_detailPane {};

    // Outer stack: browse view (results + DetailPane) vs series focus view.
    QStackedWidget* m_viewStack {};
    QWidget* m_browseView {};
    SeriesFocusView* m_focusView {};

    // Concurrency guard — increments on each new query so stale coroutines
    // can detect they've been superseded and bail out.
    quint64 m_queryEpoch = 0;
    quint64 m_detailEpoch = 0;
    quint64 m_episodeEpoch = 0;

    // Cached series context so we can re-fetch streams when the user
    // picks a different episode without re-fetching Cinemeta meta.
    QString m_currentSeriesImdbId;

    // In-memory RD token, loaded at startup and whenever the RD dialog saves.
    QString m_rdToken;
};

} // namespace kinema::ui
