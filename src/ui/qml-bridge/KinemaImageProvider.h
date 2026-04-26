// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QPointer>
#include <QQuickAsyncImageProvider>
#include <QString>

namespace kinema::ui {
class ImageLoader;
}

namespace kinema::ui::qml {

/**
 * Bridges Qt Quick's `image://` URL scheme to `ui::ImageLoader`.
 * Registered on the engine with provider id `"kinema"`, so QML can
 * load posters / backdrops / episode stills with:
 *
 *     Image { source: "image://kinema/poster?u=https%3A%2F%2F…" }
 *
 * The id is parsed as `<role>?u=<urlencoded>`. `<role>` is one of
 * `poster`, `backdrop`, `still` and currently informs only future
 * placeholder choice — `ImageLoader` itself doesn't differentiate.
 *
 * The returned `QQuickImageResponse` subscribes to the loader's
 * `posterReady` signal and resolves once the pixmap is in memory.
 * Cancellation tears down the subscription so a discarded delegate
 * doesn't keep a strong handle to the loader's signal map.
 */
class KinemaImageProvider : public QQuickAsyncImageProvider
{
public:
    explicit KinemaImageProvider(ui::ImageLoader* loader);
    ~KinemaImageProvider() override;

    QQuickImageResponse* requestImageResponse(const QString& id,
        const QSize& requestedSize) override;

private:
    QPointer<ui::ImageLoader> m_loader;
};

} // namespace kinema::ui::qml
