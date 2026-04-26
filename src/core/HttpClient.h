// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QByteArray>
#include <QJsonDocument>
#include <QNetworkAccessManager>
#include <QNetworkRequest>
#include <QObject>
#include <QString>
#include <QUrl>

#include <QCoro/QCoroTask>

#include <chrono>

namespace kinema::core {

/**
 * Thin wrapper over QNetworkAccessManager that returns QCoro::Task<T>, so
 * callers can `co_await` network responses as if they were synchronous.
 *
 * Every HttpClient owns a single QNetworkAccessManager. The app is expected
 * to create one HttpClient at process start and share it with every API
 * client (CinemetaClient, TorrentioClient, ImageLoader).
 *
 * Errors are reported by throwing HttpError from the coroutine body.
 */
class HttpClient : public QObject
{
    Q_OBJECT
public:
    explicit HttpClient(QObject* parent = nullptr);
    ~HttpClient() override = default;

    /// Issue a GET for raw bytes. Throws HttpError on failure.
    /// Virtual so tests can capture request construction without
    /// touching the real network stack.
    virtual QCoro::Task<QByteArray> get(QUrl url);

    /// Issue a GET and parse the response body as JSON. Throws HttpError on failure.
    virtual QCoro::Task<QJsonDocument> getJson(QUrl url);

    /// GET with a caller-supplied QNetworkRequest (adds our User-Agent and
    /// redirect policy on top). Used by RealDebridClient to attach the
    /// Authorization: Bearer header without re-plumbing HttpClient.
    virtual QCoro::Task<QByteArray> get(QNetworkRequest request);
    virtual QCoro::Task<QJsonDocument> getJson(QNetworkRequest request);

    /// POST a JSON body. The Content-Type header is set automatically
    /// when the caller hasn't already supplied one.
    virtual QCoro::Task<QByteArray> postJson(QNetworkRequest request,
        const QByteArray& body);
    virtual QCoro::Task<QJsonDocument> postJsonForJson(
        QNetworkRequest request, const QByteArray& body);

    /// HEAD on the given request. Returns the QNetworkReply's status
    /// code via the throwing path (HttpError on non-2xx) and the
    /// response headers map on success. Used by the moviehash compute
    /// path for `Content-Length`.
    virtual QCoro::Task<QList<QPair<QByteArray, QByteArray>>> head(
        QNetworkRequest request);

    void setTimeout(std::chrono::milliseconds timeout);
    std::chrono::milliseconds timeout() const noexcept { return m_timeout; }

    void setUserAgent(QString ua);
    const QString& userAgent() const noexcept { return m_userAgent; }

private:
    QNetworkAccessManager m_nam;
    QString m_userAgent;
    std::chrono::milliseconds m_timeout { std::chrono::seconds { 20 } };
};

} // namespace kinema::core
