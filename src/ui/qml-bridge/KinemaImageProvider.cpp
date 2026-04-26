// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/KinemaImageProvider.h"

#include "ui/ImageLoader.h"

#include <QImage>
#include <QMetaObject>
#include <QPointer>
#include <QQuickTextureFactory>
#include <QUrl>
#include <QUrlQuery>

namespace kinema::ui::qml {

namespace {

// The async response type lives in the cpp because nothing outside
// this provider needs to construct or hold one. QQuick calls async
// image providers from its pixmap-reader thread, while ImageLoader
// and the shared HttpClient live on the GUI thread. All cache/network
// interactions are therefore marshalled back to ImageLoader's thread;
// only the final QImage crosses back to this response object.
class KinemaImageResponse : public QQuickImageResponse
{
public:
    KinemaImageResponse(ui::ImageLoader* loader, const QUrl& url)
        : m_loader(loader)
        , m_url(url)
    {
        if (!loader || !url.isValid()) {
            QMetaObject::invokeMethod(this, [this] { resolve({}); },
                Qt::QueuedConnection);
            return;
        }

        // Disk-cache hits and HTTP fetches both eventually fire
        // posterReady(url). Subscribe BEFORE kicking the coroutine
        // so we never miss the signal on a fast in-process path.
        m_conn = QObject::connect(loader, &ui::ImageLoader::posterReady,
            this, [this](const QUrl& finishedUrl) {
                if (finishedUrl != m_url) {
                    return;
                }
                resolveCachedOnLoaderThread();
            }, Qt::QueuedConnection);

        // Memory-cache fast path plus fetch kickoff. This deliberately runs
        // on the loader thread because QCache is not thread-safe and the
        // fetch path touches QNetworkAccessManager through HttpClient.
        cachedOrFetchOnLoaderThread();
    }

    ~KinemaImageResponse() override
    {
        if (m_conn) {
            QObject::disconnect(m_conn);
        }
    }

    QQuickTextureFactory* textureFactory() const override
    {
        return QQuickTextureFactory::textureFactoryForImage(m_image);
    }

    void cancel() override
    {
        if (m_conn) {
            QObject::disconnect(m_conn);
            m_conn = {};
        }
        // ImageLoader has no per-request cancel API — the in-flight
        // download will complete and seed the cache for whoever
        // requests the URL next. Cheap.
        resolve({});
    }

private:
    void cachedOrFetchOnLoaderThread()
    {
        auto* loader = m_loader.data();
        if (!loader) {
            resolve({});
            return;
        }

        const QPointer<KinemaImageResponse> self(this);
        const QPointer<ui::ImageLoader> guardedLoader(loader);
        const QUrl url = m_url;
        QMetaObject::invokeMethod(loader, [self, guardedLoader, url] {
            if (!self || !guardedLoader) {
                return;
            }

            const QImage cached = guardedLoader->cached(url);
            if (!cached.isNull()) {
                QMetaObject::invokeMethod(self.data(), [self, cached] {
                    if (self) {
                        self->resolve(cached);
                    }
                }, Qt::QueuedConnection);
                return;
            }

            // Fire the fetch and discard the returned task — completion is
            // observed via posterReady above. This mirrors the app's existing
            // QCoro fire-and-forget pattern.
            auto task = guardedLoader->requestPoster(url);
            Q_UNUSED(task);
        }, Qt::QueuedConnection);
    }

    void resolveCachedOnLoaderThread()
    {
        auto* loader = m_loader.data();
        if (!loader) {
            resolve({});
            return;
        }

        const QPointer<KinemaImageResponse> self(this);
        const QPointer<ui::ImageLoader> guardedLoader(loader);
        const QUrl url = m_url;
        QMetaObject::invokeMethod(loader, [self, guardedLoader, url] {
            const QImage cached = guardedLoader ? guardedLoader->cached(url) : QImage();
            if (!self) {
                return;
            }
            QMetaObject::invokeMethod(self.data(), [self, cached] {
                if (self) {
                    self->resolve(cached);
                }
            }, Qt::QueuedConnection);
        }, Qt::QueuedConnection);
    }

    void resolve(const QImage& image)
    {
        if (m_resolved) {
            return;
        }
        m_resolved = true;
        m_image = image;
        if (m_conn) {
            QObject::disconnect(m_conn);
            m_conn = {};
        }
        Q_EMIT finished();
    }

    QPointer<ui::ImageLoader> m_loader;
    QUrl m_url;
    QImage m_image;
    QMetaObject::Connection m_conn;
    bool m_resolved {false};
};

} // namespace

KinemaImageProvider::KinemaImageProvider(ui::ImageLoader* loader)
    : m_loader(loader)
{
}

KinemaImageProvider::~KinemaImageProvider() = default;

QQuickImageResponse* KinemaImageProvider::requestImageResponse(
    const QString& id, const QSize& /*requestedSize*/)
{
    // `id` is everything after `image://kinema/`. Format:
    //   <role>?u=<urlencoded source URL>
    // We accept the role for forwards-compat (poster / backdrop /
    // still drive future placeholder choice) and ignore it for now.
    const auto qmark = id.indexOf(QLatin1Char('?'));
    QString query = qmark < 0 ? QString {} : id.mid(qmark + 1);
    const QUrlQuery q(query);

    const auto src = QUrl::fromPercentEncoding(
        q.queryItemValue(QStringLiteral("u"),
            QUrl::FullyEncoded).toUtf8());
    const QUrl url(src);

    return new KinemaImageResponse(m_loader, url);
}

} // namespace kinema::ui::qml
