// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/settings/IndexerSectionViewModelBase.h"

#include "api/indexers/IndexerSelector.h"
#include "domain/Indexer.h"
#include "kinema_log_ui.h"
#include "ui/qml-bridge/settings/SettingsStatus.h"

#include <KLocalizedString>

namespace kinema::ui::qml::settings {

IndexerSectionViewModelBase::IndexerSectionViewModelBase(api::IndexerSelector* indexers,
                                                         QObject* parent)
    : QObject(parent), m_indexers(indexers)
{ }

void IndexerSectionViewModelBase::setStatus(const QString& message, int kind)
{
    if (m_statusMessage == message && m_statusKind == kind) {
        return;
    }
    m_statusMessage = message;
    m_statusKind = kind;
    Q_EMIT statusChanged();
}

void IndexerSectionViewModelBase::setBusy(bool on)
{
    if (m_busy == on) {
        return;
    }
    m_busy = on;
    Q_EMIT busyChanged();
}

void IndexerSectionViewModelBase::testConnection()
{
    auto t = testTask();
    Q_UNUSED(t);
}

QCoro::Task<void> IndexerSectionViewModelBase::testTask()
{
    auto* indexer = m_indexers ? m_indexers->find(indexerKind()) : nullptr;
    const auto name = providerName();
    if (!indexer) {
        setStatus(i18nc("@info indexer settings status", "%1 is not registered.", name),
                  kStatusError);
        co_return;
    }
    setBusy(true);
    setStatus(i18nc("@info indexer settings status, in progress", "Probing %1…", name),
              kStatusInfo);
    const bool ok = co_await indexer->testConnection();
    if (ok) {
        setStatus(i18nc("@info indexer settings status", "%1 is reachable.", name),
                  kStatusPositive);
    } else {
        setStatus(i18nc("@info indexer settings status",
                        "%1 did not respond. Check the base URL or try again later.",
                        name),
                  kStatusError);
    }
    setBusy(false);
}

} // namespace kinema::ui::qml::settings
