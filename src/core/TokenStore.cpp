// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/TokenStore.h"

#include "kinema_debug.h"

#include <KLocalizedString>

#include <QCoro/QCoroSignal>

#include <qt6keychain/keychain.h>

#include <memory>

namespace kinema::core {

namespace {

// Await the finished(Job*) signal and take ownership of the job so it's
// torn down deterministically regardless of how the coroutine exits.
QCoro::Task<QKeychain::Job*> runJob(QKeychain::Job* job)
{
    job->setAutoDelete(false);
    job->start();
    co_await qCoro(job, &QKeychain::Job::finished);
    co_return job;
}

// Build an i18n-friendly error message from a failed job.
QString describeError(QKeychain::Job* job)
{
    switch (job->error()) {
    case QKeychain::NoBackendAvailable:
        return i18nc("@info",
            "No system keyring is available. Install and start "
            "kwalletd (Plasma) or gnome-keyring.");
    case QKeychain::AccessDeniedByUser:
        return i18nc("@info", "Access to the keyring was denied.");
    case QKeychain::AccessDenied:
        return i18nc("@info",
            "The keyring denied access. Unlock the wallet and try again.");
    case QKeychain::NotImplemented:
        return i18nc("@info",
            "This platform does not support secure credential storage.");
    case QKeychain::CouldNotDeleteEntry:
        return i18nc("@info", "Could not remove the existing keyring entry.");
    default:
        return job->errorString();
    }
}

} // namespace

TokenStore::TokenStore(QObject* parent)
    : QObject(parent)
{
}

QCoro::Task<QString> TokenStore::read(QString key)
{
    auto* raw = new QKeychain::ReadPasswordJob(
        QString::fromLatin1(kServiceName));
    raw->setKey(std::move(key));

    auto* job = co_await runJob(raw);
    const std::unique_ptr<QKeychain::Job> guard(job);

    const auto err = job->error();
    if (err == QKeychain::NoError) {
        m_available = true;
        co_return static_cast<QKeychain::ReadPasswordJob*>(job)->textData();
    }
    if (err == QKeychain::EntryNotFound) {
        // Not an error — the user has never stored this key.
        co_return QString {};
    }

    if (err == QKeychain::NoBackendAvailable) {
        m_available = false;
    }
    qCWarning(KINEMA) << "keyring read failed:" << err << job->errorString();
    throw TokenStoreError(describeError(job));
}

QCoro::Task<void> TokenStore::write(QString key, QString value)
{
    auto* raw = new QKeychain::WritePasswordJob(
        QString::fromLatin1(kServiceName));
    raw->setKey(std::move(key));
    raw->setTextData(std::move(value));

    auto* job = co_await runJob(raw);
    const std::unique_ptr<QKeychain::Job> guard(job);

    const auto err = job->error();
    if (err == QKeychain::NoError) {
        m_available = true;
        co_return;
    }
    if (err == QKeychain::NoBackendAvailable) {
        m_available = false;
    }
    qCWarning(KINEMA) << "keyring write failed:" << err << job->errorString();
    throw TokenStoreError(describeError(job));
}

QCoro::Task<void> TokenStore::remove(QString key)
{
    auto* raw = new QKeychain::DeletePasswordJob(
        QString::fromLatin1(kServiceName));
    raw->setKey(std::move(key));

    auto* job = co_await runJob(raw);
    const std::unique_ptr<QKeychain::Job> guard(job);

    const auto err = job->error();
    if (err == QKeychain::NoError || err == QKeychain::EntryNotFound) {
        m_available = true;
        co_return;
    }
    if (err == QKeychain::NoBackendAvailable) {
        m_available = false;
    }
    qCWarning(KINEMA) << "keyring remove failed:" << err << job->errorString();
    throw TokenStoreError(describeError(job));
}

} // namespace kinema::core
