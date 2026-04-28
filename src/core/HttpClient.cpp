// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/HttpClient.h"

#include "core/HttpError.h"
#include "core/UrlRedactor.h"
#include "kinema_debug.h"
#include "kinema_version.h"

#include <QCoro/QCoroNetworkReply>

#include <KLocalizedString>

#include <QJsonParseError>
#include <QNetworkReply>
#include <QNetworkRequest>

namespace kinema::core {

namespace {

HttpError::Kind kindFromReply(QNetworkReply* reply) noexcept
{
    return reply->error() == QNetworkReply::OperationCanceledError
        ? HttpError::Kind::Cancelled
        : HttpError::Kind::Network;
}

QJsonDocument parseJsonOrThrow(const QByteArray& body)
{
    QJsonParseError perr;
    auto doc = QJsonDocument::fromJson(body, &perr);
    if (perr.error != QJsonParseError::NoError) {
        throw HttpError(HttpError::Kind::Json, 0,
            i18n("Could not parse JSON response: %1", perr.errorString()));
    }
    return doc;
}

void requireHttps(const QUrl& url)
{
    if (url.scheme() != QLatin1String("https")) {
        throw HttpError(HttpError::Kind::Network, 0,
            i18n("Refusing non-HTTPS URL: %1", url.toString()));
    }
}

} // namespace

HttpClient::HttpClient(QObject* parent)
    : QObject(parent)
    , m_userAgent(QStringLiteral("Kinema/%1").arg(QStringLiteral(KINEMA_VERSION_STRING)))
{
    m_nam.setTransferTimeout(m_timeout);
    m_nam.setAutoDeleteReplies(false);
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
    co_return co_await get(QNetworkRequest(url));
}

QCoro::Task<QJsonDocument> HttpClient::getJson(QUrl url)
{
    co_return co_await getJson(QNetworkRequest(url));
}

QCoro::Task<QByteArray> HttpClient::get(QNetworkRequest request)
{
    requireHttps(request.url());
    if (!request.hasRawHeader("User-Agent")) {
        request.setHeader(QNetworkRequest::UserAgentHeader, m_userAgent);
    }
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute,
        QNetworkRequest::NoLessSafeRedirectPolicy);

    const QString logUrl = redactUrlForLog(request.url());
    qCDebug(KINEMA) << "HTTP GET" << logUrl;

    QNetworkReply* reply = m_nam.get(request);
    co_await qCoro(reply).waitForFinished();
    const std::unique_ptr<QNetworkReply> replyGuard { reply };

    const auto status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();

    if (reply->error() != QNetworkReply::NoError) {
        qCWarning(KINEMA) << "HTTP failed" << logUrl
                          << reply->error() << reply->errorString();
        throw HttpError(kindFromReply(reply), status, reply->errorString());
    }

    if (status < 200 || status >= 300) {
        throw HttpError(HttpError::Kind::HttpStatus, status,
            i18n("Server returned HTTP %1 for %2", status, logUrl));
    }

    co_return reply->readAll();
}

QCoro::Task<QJsonDocument> HttpClient::getJson(QNetworkRequest request)
{
    const QByteArray body = co_await get(std::move(request));
    co_return parseJsonOrThrow(body);
}

QCoro::Task<QByteArray> HttpClient::postJson(QNetworkRequest request,
    const QByteArray& body)
{
    requireHttps(request.url());
    if (!request.hasRawHeader("User-Agent")) {
        request.setHeader(QNetworkRequest::UserAgentHeader, m_userAgent);
    }
    if (!request.hasRawHeader("Content-Type")) {
        request.setHeader(QNetworkRequest::ContentTypeHeader,
            QStringLiteral("application/json"));
    }
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute,
        QNetworkRequest::NoLessSafeRedirectPolicy);

    const QString logUrl = redactUrlForLog(request.url());
    qCDebug(KINEMA) << "HTTP POST" << logUrl;

    QNetworkReply* reply = m_nam.post(request, body);
    co_await qCoro(reply).waitForFinished();
    const std::unique_ptr<QNetworkReply> replyGuard { reply };
    const auto status = reply->attribute(
        QNetworkRequest::HttpStatusCodeAttribute).toInt();

    if (reply->error() != QNetworkReply::NoError) {
        const auto respBody = QString::fromUtf8(reply->readAll());
        const auto msg = respBody.isEmpty() ? reply->errorString() : respBody;
        qCWarning(KINEMA) << "HTTP POST failed" << logUrl
                          << reply->error() << msg;
        throw HttpError(kindFromReply(reply), status, msg);
    }
    if (status < 200 || status >= 300) {
        const auto respBody = QString::fromUtf8(reply->readAll());
        const auto msg = respBody.isEmpty()
            ? i18n("Server returned HTTP %1 for %2", status, logUrl)
            : respBody;
        throw HttpError(HttpError::Kind::HttpStatus, status, msg);
    }
    co_return reply->readAll();
}

QCoro::Task<QJsonDocument> HttpClient::postJsonForJson(
    QNetworkRequest request, const QByteArray& body)
{
    const QByteArray respBody = co_await postJson(std::move(request), body);
    co_return parseJsonOrThrow(respBody);
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

    const QString logUrl = redactUrlForLog(url);
    qCDebug(KINEMA) << "HTTP HEAD" << logUrl;

    QNetworkReply* reply = m_nam.head(request);
    co_await qCoro(reply).waitForFinished();
    const std::unique_ptr<QNetworkReply> replyGuard { reply };
    const auto status = reply->attribute(
        QNetworkRequest::HttpStatusCodeAttribute).toInt();

    if (reply->error() != QNetworkReply::NoError) {
        throw HttpError(kindFromReply(reply), status, reply->errorString());
    }
    if (status < 200 || status >= 300) {
        throw HttpError(HttpError::Kind::HttpStatus, status,
            i18n("Server returned HTTP %1 for %2", status, logUrl));
    }
    co_return reply->rawHeaderPairs();
}

} // namespace kinema::core
