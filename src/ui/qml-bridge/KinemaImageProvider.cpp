// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/KinemaImageProvider.h"

#include "ui/ImageLoader.h"

#include <QCoro/QCoroTask>

#include <QImage>
#include <QPixmap>
#include <QQuickTextureFactory>
#include <QUrl>
#include <QUrlQuery>

namespace kinema::ui::qml {

namespace {

// The async response type lives in the cpp because nothing outside
// this provider needs to construct or hold one. It owns the
// connection back to ImageLoader and resolves itself when the
// pixmap arrives.
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

        // Memory-cache fast path: avoids spinning the coroutine and
        // a queued signal trip just to return an already-decoded
        // pixmap.
        const auto cached = loader->cached(url);
        if (!cached.isNull()) {
            QMetaObject::invokeMethod(this, [this, cached] { resolve(cached); },
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
                if (!m_loader) {
                    resolve({});
                    return;
                }
                resolve(m_loader->cached(m_url));
            });

        // Fire the fetch and discard the returned task — completion
        // is observed via `posterReady` above. QCoro's task is
        // started eagerly when the caller doesn't co_await, so this
        // begins immediately.
        loader->requestPoster(url);
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
    void resolve(const QPixmap& pm)
    {
        if (m_resolved) {
            return;
        }
        m_resolved = true;
        if (!pm.isNull()) {
            m_image = pm.toImage();
        }
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
