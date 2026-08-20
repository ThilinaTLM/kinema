// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "api/AllDebridClient.h"

#include "api/AllDebridParse.h"
#include "api/RequestUtil.h"
#include "core/io/HttpClient.h"
#include "core/io/HttpError.h"

#include <KLocalizedString>

#include <QCoro/QCoroSignal>
#include <QTimer>

namespace kinema::api {
using namespace kinema::domain;

namespace {

constexpr int kDelayedPollMs = 5'000;
constexpr int kDelayedTimeoutMs = 90'000;

void requireApiKey(const QString& key)
{
    if (key.isEmpty()) {
        throw core::HttpError(core::HttpError::Kind::HttpStatus, 401,
            i18n("No AllDebrid API key set."));
    }
}

QCoro::Task<void> sleepMs(int ms)
{
    QTimer t;
    t.setSingleShot(true);
    t.start(ms);
    co_await qCoro(&t, &QTimer::timeout);
}

} // namespace

AllDebridClient::AllDebridClient(core::HttpClient* http, QObject* parent)
    : QObject(parent)
    , m_http(http)
    , m_baseUrl(QStringLiteral("https://api.alldebrid.com"))
{
}

void AllDebridClient::setBaseUrl(QUrl url)
{
    m_baseUrl = std::move(url);
}

void AllDebridClient::setApiKey(QString apiKey)
{
    m_apiKey = std::move(apiKey);
}

QCoro::Task<AllDebridUser> AllDebridClient::user()
{
    requireApiKey(m_apiKey);

    QNetworkRequest req(appendPath(m_baseUrl, QStringLiteral("/v4/user")));
    req.setRawHeader("Authorization", bearer(m_apiKey));

    const auto doc = co_await m_http->getJson(std::move(req));
    co_return alldebrid::parseUser(doc);
}

QCoro::Task<AdAddMagnetResult> AllDebridClient::uploadMagnet(QString magnetUri)
{
    requireApiKey(m_apiKey);
    if (magnetUri.isEmpty()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("AllDebrid magnet/upload: empty magnet URI."));
    }

    auto req = bearerFormPost(
        appendPath(m_baseUrl, QStringLiteral("/v4/magnet/upload")), bearer(m_apiKey));
    const auto body = urlEncode({
        { QStringLiteral("magnets[]"), magnetUri },
    });
    const auto doc = co_await m_http->postJsonForJson(std::move(req), body);
    co_return alldebrid::parseAddMagnet(doc);
}

QCoro::Task<AdMagnetStatus> AllDebridClient::magnetStatus(qint64 magnetId)
{
    requireApiKey(m_apiKey);
    if (magnetId <= 0) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("AllDebrid magnet/status: invalid magnet id."));
    }

    auto req = bearerFormPost(
        appendPath(m_baseUrl, QStringLiteral("/v4.1/magnet/status")), bearer(m_apiKey));
    const auto body = urlEncode({
        { QStringLiteral("id"), QString::number(magnetId) },
    });
    const auto doc = co_await m_http->postJsonForJson(std::move(req), body);
    co_return alldebrid::parseMagnetStatus(doc);
}

QCoro::Task<QList<AdMagnetFile>> AllDebridClient::magnetFiles(qint64 magnetId)
{
    requireApiKey(m_apiKey);
    if (magnetId <= 0) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("AllDebrid magnet/files: invalid magnet id."));
    }

    auto req = bearerFormPost(
        appendPath(m_baseUrl, QStringLiteral("/v4/magnet/files")), bearer(m_apiKey));
    const auto body = urlEncode({
        { QStringLiteral("id[]"), QString::number(magnetId) },
    });
    const auto doc = co_await m_http->postJsonForJson(std::move(req), body);
    co_return alldebrid::parseMagnetFiles(doc);
}

QCoro::Task<AdUnlockedLink> AllDebridClient::unlockLink(QUrl link)
{
    requireApiKey(m_apiKey);
    if (link.isEmpty()) {
        throw core::HttpError(core::HttpError::Kind::Json, 0,
            i18n("AllDebrid link/unlock: empty link."));
    }

    auto req = bearerFormPost(
        appendPath(m_baseUrl, QStringLiteral("/v4/link/unlock")), bearer(m_apiKey));
    const auto body = urlEncode({
        { QStringLiteral("link"), link.toString() },
    });
    const auto doc = co_await m_http->postJsonForJson(std::move(req), body);
    auto unlocked = alldebrid::parseUnlock(doc);
    if (!unlocked.download.isEmpty()) {
        co_return unlocked;
    }

    // Delayed flow.
    auto resolved = co_await pollDelayed(unlocked.delayedId);
    if (resolved.filename.isEmpty()) {
        resolved.filename = unlocked.filename;
    }
    if (resolved.fileSize <= 0) {
        resolved.fileSize = unlocked.fileSize;
    }
    if (resolved.host.isEmpty()) {
        resolved.host = unlocked.host;
    }
    co_return resolved;
}

QCoro::Task<AdUnlockedLink> AllDebridClient::pollDelayed(qint64 delayedId)
{
    int waitedMs = 0;
    while (true) {
        auto req = bearerFormPost(
            appendPath(m_baseUrl, QStringLiteral("/v4/link/delayed")), bearer(m_apiKey));
        const auto body = urlEncode({
            { QStringLiteral("id"), QString::number(delayedId) },
        });
        const auto doc = co_await m_http->postJsonForJson(std::move(req), body);
        const auto u = alldebrid::parseDelayed(doc);
        if (!u.download.isEmpty()) {
            co_return u;
        }
        if (waitedMs >= kDelayedTimeoutMs) {
            throw core::HttpError(core::HttpError::Kind::HttpStatus, 504,
                i18n("AllDebrid link/delayed timed out waiting for the URL."));
        }
        co_await sleepMs(kDelayedPollMs);
        waitedMs += kDelayedPollMs;
    }
}

QCoro::Task<void> AllDebridClient::deleteMagnet(qint64 magnetId)
{
    requireApiKey(m_apiKey);
    if (magnetId <= 0) {
        co_return;
    }

    auto req = bearerFormPost(
        appendPath(m_baseUrl, QStringLiteral("/v4/magnet/delete")), bearer(m_apiKey));
    const auto body = urlEncode({
        { QStringLiteral("id"), QString::number(magnetId) },
    });
    co_await m_http->postJson(std::move(req), body);
    co_return;
}

} // namespace kinema::api
