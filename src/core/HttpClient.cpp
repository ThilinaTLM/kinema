// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/HttpClient.h"

#include "core/HttpError.h"
#include "kinema_debug.h"
#include "kinema_version.h"

#include <QCoro/QCoroNetworkReply>

#include <KLocalizedString>

#include <QJsonParseError>
#include <QNetworkReply>
#include <QNetworkRequest>

namespace kinema::core {

HttpClient::HttpClient(QObject* parent)
    : QObject(parent)
    , m_userAgent(QStringLiteral("Kinema/%1").arg(QStringLiteral(KINEMA_VERSION_STRING)))
{
    m_nam.setTransferTimeout(m_timeout);
    m_nam.setAutoDeleteReplies(false); // replies are deleted explicitly after co_await
    m_nam.setRedirectPolicy(QNetworkRequest::NoLessSafeRedirectPolicy);
}

void HttpClient::setTimeout(std::chrono::milliseconds timeout)
{
    m_timeout = timeout;
    m_nam.setTransferTimeout(m_timeout);
}

void HttpClient::setUserAgent(QString ua)
{
    m_userAgent = std::move(ua);
}

QCoro::Task<QByteArray> HttpClient::get(QUrl url)
{
    QNetworkRequest request(url);
    co_return co_await get(std::move(request));
}

QCoro::Task<QJsonDocument> HttpClient::getJson(QUrl url)
{
    QNetworkRequest request(url);
    co_return co_await getJson(std::move(request));
}

QCoro::Task<QByteArray> HttpClient::get(QNetworkRequest request)
{
    const QUrl url = request.url();
    if (url.scheme() != QLatin1String("https")) {
        throw HttpError(HttpError::Kind::Network, 0,
            i18n("Refusing non-HTTPS URL: %1", url.toString()));
    }

    // Inject our defaults without clobbering caller-set headers.
    if (!request.hasRawHeader("User-Agent")) {
        request.setHeader(QNetworkRequest::UserAgentHeader, m_userAgent);
    }
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute,
        QNetworkRequest::NoLessSafeRedirectPolicy);

    qCDebug(KINEMA) << "HTTP GET" << url.toString(QUrl::RemoveQuery);

    QNetworkReply* reply = m_nam.get(request);
    co_await qCoro(reply).waitForFinished();

    // Take ownership so the reply is released regardless of how we exit.
    const std::unique_ptr<QNetworkReply> replyGuard { reply };

    const auto status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();

    if (reply->error() != QNetworkReply::NoError) {
        // Map OperationCanceledError to Cancelled so callers can distinguish.
        const auto kind = reply->error() == QNetworkReply::OperationCanceledError
            ? HttpError::Kind::Cancelled
            : HttpError::Kind::Network;
        qCWarning(KINEMA) << "HTTP failed" << url.toString(QUrl::RemoveQuery)
                          << reply->error() << reply->errorString();
        throw HttpError(kind, status, reply->errorString());
    }

    if (status < 200 || status >= 300) {
        throw HttpError(HttpError::Kind::HttpStatus, status,
            i18n("Server returned HTTP %1 for %2", status, url.toString(QUrl::RemoveQuery)));
    }

    co_return reply->readAll();
}

QCoro::Task<QJsonDocument> HttpClient::getJson(QNetworkRequest request)
{
    const QByteArray body = co_await get(std::move(request));

    QJsonParseError perr;
    auto doc = QJsonDocument::fromJson(body, &perr);
    if (perr.error != QJsonParseError::NoError) {
        throw HttpError(HttpError::Kind::Json, 0,
            i18n("Could not parse JSON response: %1", perr.errorString()));
    }
    co_return doc;
}

QCoro::Task<QByteArray> HttpClient::postJson(QNetworkRequest request,
    const QByteArray& body)
{
    const QUrl url = request.url();
    if (url.scheme() != QLatin1String("https")) {
        throw HttpError(HttpError::Kind::Network, 0,
            i18n("Refusing non-HTTPS URL: %1", url.toString()));
    }
    if (!request.hasRawHeader("User-Agent")) {
        request.setHeader(QNetworkRequest::UserAgentHeader, m_userAgent);
    }
    if (!request.hasRawHeader("Content-Type")) {
        request.setHeader(QNetworkRequest::ContentTypeHeader,
            QStringLiteral("application/json"));
    }
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute,
        QNetworkRequest::NoLessSafeRedirectPolicy);

    qCDebug(KINEMA) << "HTTP POST" << url.toString(QUrl::RemoveQuery);

    QNetworkReply* reply = m_nam.post(request, body);
    co_await qCoro(reply).waitForFinished();
    const std::unique_ptr<QNetworkReply> replyGuard { reply };
    const auto status = reply->attribute(
        QNetworkRequest::HttpStatusCodeAttribute).toInt();

    if (reply->error() != QNetworkReply::NoError) {
        const auto kind = reply->error() == QNetworkReply::OperationCanceledError
            ? HttpError::Kind::Cancelled
            : HttpError::Kind::Network;
        // Surface the response body when the server returned one — it
        // usually carries a structured error message.
        const auto respBody = QString::fromUtf8(reply->readAll());
        const auto msg = respBody.isEmpty()
            ? reply->errorString()
            : respBody;
        qCWarning(KINEMA) << "HTTP POST failed"
                          << url.toString(QUrl::RemoveQuery)
                          << reply->error() << msg;
        throw HttpError(kind, status, msg);
    }
    if (status < 200 || status >= 300) {
        const auto respBody = QString::fromUtf8(reply->readAll());
        const auto msg = respBody.isEmpty()
            ? i18n("Server returned HTTP %1 for %2", status,
                  url.toString(QUrl::RemoveQuery))
            : respBody;
        throw HttpError(HttpError::Kind::HttpStatus, status, msg);
    }
    co_return reply->readAll();
}

QCoro::Task<QJsonDocument> HttpClient::postJsonForJson(
    QNetworkRequest request, const QByteArray& body)
{
    const QByteArray respBody = co_await postJson(std::move(request), body);
    QJsonParseError perr;
    auto doc = QJsonDocument::fromJson(respBody, &perr);
    if (perr.error != QJsonParseError::NoError) {
        throw HttpError(HttpError::Kind::Json, 0,
            i18n("Could not parse JSON response: %1", perr.errorString()));
    }
    co_return doc;
}

QCoro::Task<QList<QPair<QByteArray, QByteArray>>> HttpClient::head(
    QNetworkRequest request)
{
    const QUrl url = request.url();
    if (url.scheme() != QLatin1String("https")
        && url.scheme() != QLatin1String("http")) {
        throw HttpError(HttpError::Kind::Network, 0,
            i18n("Unsupported URL scheme: %1", url.toString()));
    }
    if (!request.hasRawHeader("User-Agent")) {
        request.setHeader(QNetworkRequest::UserAgentHeader, m_userAgent);
    }
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute,
        QNetworkRequest::NoLessSafeRedirectPolicy);

    qCDebug(KINEMA) << "HTTP HEAD" << url.toString(QUrl::RemoveQuery);

    QNetworkReply* reply = m_nam.head(request);
    co_await qCoro(reply).waitForFinished();
    const std::unique_ptr<QNetworkReply> replyGuard { reply };
    const auto status = reply->attribute(
        QNetworkRequest::HttpStatusCodeAttribute).toInt();

    if (reply->error() != QNetworkReply::NoError) {
        const auto kind = reply->error() == QNetworkReply::OperationCanceledError
            ? HttpError::Kind::Cancelled
            : HttpError::Kind::Network;
        throw HttpError(kind, status, reply->errorString());
    }
    if (status < 200 || status >= 300) {
        throw HttpError(HttpError::Kind::HttpStatus, status,
            i18n("Server returned HTTP %1 for %2", status,
                url.toString(QUrl::RemoveQuery)));
    }
    co_return reply->rawHeaderPairs();
}

} // namespace kinema::core
