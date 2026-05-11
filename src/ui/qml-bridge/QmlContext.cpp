// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/QmlContext.h"

#include "app/ServiceContainer.h"
#include "ui/ImageLoader.h"
#include "ui/qml-bridge/AppIconResolver.h"
#include "ui/qml-bridge/BrowseViewModel.h"
#include "ui/qml-bridge/ContinueWatchingViewModel.h"
#include "ui/qml-bridge/DiscoverSectionModel.h"
#include "ui/qml-bridge/DiscoverViewModel.h"
#include "ui/qml-bridge/DownloadsListModel.h"
#include "ui/qml-bridge/DownloadsViewModel.h"
#include "ui/qml-bridge/EpisodesListModel.h"
#include "ui/qml-bridge/KinemaImageProvider.h"
#include "ui/qml-bridge/LibraryListModel.h"
#include "ui/qml-bridge/LibraryRailModel.h"
#include "ui/qml-bridge/LibraryViewModel.h"
#include "ui/qml-bridge/MovieDetailViewModel.h"
#include "ui/qml-bridge/ResultsListModel.h"
#include "ui/qml-bridge/SearchViewModel.h"
#include "ui/qml-bridge/SeriesDetailViewModel.h"
#include "ui/qml-bridge/ShellViewModel.h"
#include "ui/qml-bridge/StreamsListModel.h"
#include "ui/qml-bridge/SubtitleResultsModel.h"
#include "ui/qml-bridge/SubtitlesViewModel.h"
#include "ui/qml-bridge/settings/AllDebridSectionViewModel.h"
#include "ui/qml-bridge/settings/DebridSettingsViewModel.h"
#include "ui/qml-bridge/settings/GeneralSettingsViewModel.h"
#include "ui/qml-bridge/settings/IndexerSettingsViewModel.h"
#include "ui/qml-bridge/settings/PeerflixSectionViewModel.h"
#include "ui/qml-bridge/settings/PlayerSettingsViewModel.h"
#include "ui/qml-bridge/settings/RealDebridSectionViewModel.h"
#include "ui/qml-bridge/settings/SettingsRootViewModel.h"
#include "ui/qml-bridge/settings/StreamsSettingsViewModel.h"
#include "ui/qml-bridge/settings/SubtitlesSettingsViewModel.h"
#include "ui/qml-bridge/settings/TmdbSettingsViewModel.h"
#include "ui/qml-bridge/settings/TorrentStreamingSettingsViewModel.h"
#include "ui/qml-bridge/settings/TorrentioSectionViewModel.h"

#include <KLocalizedContext>

#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQmlEngine>
#include <QString>

namespace kinema::ui::qml {

namespace {

void registerQmlTypes()
{
    // Register the section / list / view-model classes as
    // uncreatable QML types so QML can read their `State` enums
    // by name (e.g. `DiscoverSectionModel.Loading`). Runtime
    // registration scoped to the engine's type system; does NOT
    // route through `QML_ELEMENT` so it stays compatible with the
    // kinema_core / kinema_qml_app target split. Calling it once
    // per engine is fine; Qt deduplicates.
#define KINEMA_REGISTER_QML_TYPE(Type) \
    qmlRegisterUncreatableType<Type>("dev.tlmtech.kinema.app", 1, 0, \
        #Type, QStringLiteral(#Type " is owned by C++."))

    KINEMA_REGISTER_QML_TYPE(DiscoverSectionModel);
    KINEMA_REGISTER_QML_TYPE(ResultsListModel);
    KINEMA_REGISTER_QML_TYPE(StreamsListModel);
    KINEMA_REGISTER_QML_TYPE(LibraryListModel);
    KINEMA_REGISTER_QML_TYPE(LibraryRailModel);
    KINEMA_REGISTER_QML_TYPE(LibraryViewModel);
    KINEMA_REGISTER_QML_TYPE(MovieDetailViewModel);
    KINEMA_REGISTER_QML_TYPE(SeriesDetailViewModel);
    KINEMA_REGISTER_QML_TYPE(EpisodesListModel);
    KINEMA_REGISTER_QML_TYPE(SubtitlesViewModel);
    KINEMA_REGISTER_QML_TYPE(SubtitleResultsModel);
    KINEMA_REGISTER_QML_TYPE(DownloadsViewModel);
    KINEMA_REGISTER_QML_TYPE(DownloadsListModel);
    KINEMA_REGISTER_QML_TYPE(settings::SettingsRootViewModel);
    KINEMA_REGISTER_QML_TYPE(settings::GeneralSettingsViewModel);
    KINEMA_REGISTER_QML_TYPE(settings::TmdbSettingsViewModel);
    KINEMA_REGISTER_QML_TYPE(settings::RealDebridSectionViewModel);
    KINEMA_REGISTER_QML_TYPE(settings::AllDebridSectionViewModel);
    KINEMA_REGISTER_QML_TYPE(settings::DebridSettingsViewModel);
    KINEMA_REGISTER_QML_TYPE(settings::IndexerSettingsViewModel);
    KINEMA_REGISTER_QML_TYPE(settings::TorrentioSectionViewModel);
    KINEMA_REGISTER_QML_TYPE(settings::PeerflixSectionViewModel);
    KINEMA_REGISTER_QML_TYPE(settings::StreamsSettingsViewModel);
    KINEMA_REGISTER_QML_TYPE(settings::PlayerSettingsViewModel);
    KINEMA_REGISTER_QML_TYPE(settings::SubtitlesSettingsViewModel);
    KINEMA_REGISTER_QML_TYPE(settings::TorrentStreamingSettingsViewModel);

#undef KINEMA_REGISTER_QML_TYPE
}

} // namespace

void installQmlContext(QQmlApplicationEngine& engine,
    app::ServiceContainer& services, ShellViewModel& shell)
{
    // QQmlEngine takes ownership of the provider. Provider id is
    // referenced from QML as `image://kinema/<id>`.
    engine.addImageProvider(QStringLiteral("kinema"),
        new KinemaImageProvider(services.imageLoader()));

    registerQmlTypes();

    // The app-icon resolver is a singleton accessible from QML by
    // module URI. Container creates it lazily on first call.
    qmlRegisterSingletonInstance("dev.tlmtech.kinema.app", 1, 0,
        "AppIconResolver", services.appIconResolver());

    // Wire `KLocalizedContext` so QML can call `i18n(...)` /
    // `i18nc(...)` and have them route through the same KCatalog
    // that `KLocalizedString` uses on the C++ side. Owned by the
    // engine's root context.
    auto* localized = new KLocalizedContext(&engine);
    engine.rootContext()->setContextObject(localized);

    auto* rootCtx = engine.rootContext();
    const std::pair<const char*, QObject*> contextProps[] = {
        { "shell", &shell },
        { "discoverVm", services.discoverVm() },
        { "continueWatchingVm", services.continueWatchingVm() },
        { "libraryVm", services.libraryVm() },
        { "searchVm", services.searchVm() },
        { "browseVm", services.browseVm() },
        { "movieDetailVm", services.movieDetailVm() },
        { "seriesDetailVm", services.seriesDetailVm() },
        { "subtitlesVm", services.subtitlesVm() },
        { "settingsVm", services.settingsVm() },
        { "downloadsVm", services.downloadsVm() },
    };
    for (const auto& [name, obj] : contextProps) {
        rootCtx->setContextProperty(QString::fromLatin1(name), obj);
    }

    // Kick the initial Discover + Browse fetches once everything's
    // wired so each page lands populated rather than empty-spinner.
    // Token resolution may finish later via `loadAll()`; both VMs
    // listen on `tmdbTokenChanged` for a delayed-arrival refresh.
    services.discoverVm()->refresh();
    services.browseVm()->refresh();
}

} // namespace kinema::ui::qml
