// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>
#include <QString>

#include <memory>

class QQmlApplicationEngine;

namespace kinema::config {
class AppSettings;
}

namespace kinema::core {
class HttpClient;
}

namespace kinema::ui {
class ImageLoader;
}

namespace kinema::ui::qml {

/**
 * Top-level QML host. Owns the app-side HTTP stack and image
 * loader, registers the `image://kinema/...` provider on the QML
 * engine, and exposes itself as the `mainController` context
 * property so QML pages can read shared application data.
 *
 * Phase 01 surface is intentionally minimal — just `applicationName`
 * for the about / window title placeholders. Later phases grow this
 * into the canonical owner of every controller / view-model that
 * `MainWindow` currently houses (navigation, search, detail,
 * settings, tray, …).
 *
 * Single responsibility: stand in for `MainWindow`'s composition
 * role. UI lives entirely in QML; this class never touches widgets.
 */
class MainController : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString applicationName READ applicationName CONSTANT)

public:
    explicit MainController(config::AppSettings& settings,
        QQmlApplicationEngine& engine, QObject* parent = nullptr);
    ~MainController() override;

    QString applicationName() const;

    /// Shared HTTP client used by the app-side image provider.
    /// Phase 02+ surfaces this to QML view-models.
    core::HttpClient* httpClient() const { return m_http.get(); }

    /// Shared image loader the QML image provider proxies for.
    /// Phase 02+ surfaces this to QML view-models that drive
    /// poster / backdrop URLs.
    ui::ImageLoader* imageLoader() const { return m_imageLoader; }

private:
    void buildCoreServices();
    void registerImageProvider(QQmlApplicationEngine& engine);
    void exposeContextProperties(QQmlApplicationEngine& engine);

    config::AppSettings& m_settings;

    std::unique_ptr<core::HttpClient> m_http;
    ui::ImageLoader* m_imageLoader {};
};

} // namespace kinema::ui::qml
