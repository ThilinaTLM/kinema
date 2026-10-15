// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <KSharedConfig>

#include <QObject>
#include <QStringList>

namespace kinema::config {

/**
 * Subtitles preferences. Tokens / credentials live in
 * `core::TokenStore`, never in `KSharedConfig`.
 *
 * KConfig group: `[Subtitles]`
 *
 * Keys:
 *   preferredLanguages   ordered ISO 639-2 codes; empty = no filter
 *   hearingImpaired      "off" | "include" | "only"
 *   foreignPartsOnly     "off" | "include" | "only"
 */
class SubtitleSettings : public QObject
{
    Q_OBJECT
public:
    explicit SubtitleSettings(KSharedConfigPtr config,
        QObject* parent = nullptr);

    QStringList preferredLanguages() const;
    void setPreferredLanguages(const QStringList&);

    QString hearingImpaired() const;
    void setHearingImpaired(const QString&);

    QString foreignPartsOnly() const;
    void setForeignPartsOnly(const QString&);

Q_SIGNALS:
    void preferredLanguagesChanged(const QStringList&);
    void hearingImpairedChanged(const QString&);
    void foreignPartsOnlyChanged(const QString&);

private:
    KSharedConfigPtr m_config;
};

} // namespace kinema::config
