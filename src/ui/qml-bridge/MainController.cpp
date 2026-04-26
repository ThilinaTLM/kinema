// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/MainController.h"

#include "core/HttpClient.h"
#include "ui/ImageLoader.h"
#include "ui/qml-bridge/KinemaImageProvider.h"

#include <KAboutData>

#include <QQmlApplicationEngine>
#include <QQmlContext>

namespace kinema::ui::qml {

MainController::MainController(config::AppSettings& settings,
    QQmlApplicationEngine& engine, QObject* parent)
    : QObject(parent)
    , m_settings(settings)
{
    buildCoreServices();
    registerImageProvider(engine);
    exposeContextProperties(engine);
}

MainController::~MainController() = default;

QString MainController::applicationName() const
{
    // KAboutData carries the localised display name set in
    // KinemaApplication::configure(); fall back to the org name if
    // about data wasn't set up yet.
    const auto about = KAboutData::applicationData();
    if (!about.displayName().isEmpty()) {
        return about.displayName();
    }
    return QStringLiteral("Kinema");
}

void MainController::buildCoreServices()
{
    // Phase 01 owns just the HTTP client and the image loader so
    // the QML image provider has something to delegate to. Other
    // controllers / API clients land in later phases as their
    // pages get migrated; until then the (still-alive but hidden)
    // legacy MainWindow keeps its own copies.
    m_http = std::make_unique<core::HttpClient>(this);
    m_imageLoader = new ImageLoader(m_http.get(), this);
}

void MainController::registerImageProvider(QQmlApplicationEngine& engine)
{
    // QQmlEngine takes ownership of the provider. Provider id is
    // referenced from QML as `image://kinema/<id>`.
    engine.addImageProvider(QStringLiteral("kinema"),
        new KinemaImageProvider(m_imageLoader));
}

void MainController::exposeContextProperties(QQmlApplicationEngine& engine)
{
    engine.rootContext()->setContextProperty(
        QStringLiteral("mainController"), this);
}

} // namespace kinema::ui::qml
