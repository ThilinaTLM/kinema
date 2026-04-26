// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/SubtitlesViewModel.h"

#include "config/SubtitleSettings.h"
#include "controllers/SubtitleController.h"
#include "core/Language.h"
#include "ui/qml-bridge/SubtitleResultsModel.h"

#include <KLocalizedString>

#include <QFileInfo>
#include <QStringList>
#include <QUrl>

namespace kinema::ui::qml {

SubtitlesViewModel::SubtitlesViewModel(
    controllers::SubtitleController* controller,
    config::SubtitleSettings& settings, QObject* parent)
    : QObject(parent)
    , m_controller(controller)
    , m_settings(settings)
    , m_model(new SubtitleResultsModel(this))
{
    m_languages = m_settings.preferredLanguages();
    m_hi = m_settings.hearingImpaired();
    m_fpo = m_settings.foreignPartsOnly();

    if (m_controller) {
        connect(m_controller,
            &controllers::SubtitleController::hitsChanged, this,
            &SubtitlesViewModel::onHitsChanged);
        connect(m_controller,
            &controllers::SubtitleController::cacheChanged, this,
            &SubtitlesViewModel::onCacheChanged);
        connect(m_controller,
            &controllers::SubtitleController::activeSubtitleChanged,
            this, &SubtitlesViewModel::onCacheChanged);
        connect(m_controller,
            &controllers::SubtitleController::searchingChanged, this,
            &SubtitlesViewModel::onSearchingChanged);
        connect(m_controller,
            &controllers::SubtitleController::errorChanged, this,
            &SubtitlesViewModel::onErrorChanged);
        connect(m_controller,
            &controllers::SubtitleController::downloadEnabledChanged,
            this, &SubtitlesViewModel::onDownloadEnabledChanged);
        connect(m_controller,
            &controllers::SubtitleController::quotaChanged, this,
            &SubtitlesViewModel::onQuotaChanged);
        connect(m_controller,
            &controllers::SubtitleController::downloadFinished, this,
            &SubtitlesViewModel::onDownloadFinished);
        connect(m_controller,
            &controllers::SubtitleController::downloadFailed, this,
            &SubtitlesViewModel::onDownloadFailed);
    }

    updateState();
    updateStatusLine();
    updatePrimarySemantics();
}

QObject* SubtitlesViewModel::results() const
{
    return m_model;
}

QVariantList SubtitlesViewModel::commonLanguages() const
{
    QVariantList out;
    for (const auto& opt : core::language::commonLanguages()) {
        QVariantMap row;
        row.insert(QStringLiteral("code"), opt.code);
        row.insert(QStringLiteral("display"), opt.display);
        out.append(row);
    }
    return out;
}

QString SubtitlesViewModel::languageDisplayName(const QString& code) const
{
    return core::language::displayName(code);
}

QString SubtitlesViewModel::state() const
{
    return m_state;
}

QString SubtitlesViewModel::errorText() const
{
    return m_controller ? m_controller->lastError() : QString {};
}

void SubtitlesViewModel::setMedia(const api::PlaybackContext& ctx)
{
    m_context = ctx;
    m_contextTitle = formatContextTitle(ctx);
    Q_EMIT contextChanged();

    // Reset filters from settings on every open so a previous
    // session's release filter doesn't carry over to a different
    // title.
    m_languages = m_settings.preferredLanguages();
    m_hi = m_settings.hearingImpaired();
    m_fpo = m_settings.foreignPartsOnly();
    m_release.clear();
    Q_EMIT filtersChanged();

    m_model->setHits({});
    m_selectedRow = -1;
    Q_EMIT selectedRowChanged();

    updateState();
    updateStatusLine();
    updatePrimarySemantics();

    // Auto-kick a search when configured + we have a valid key.
    if (m_controller && m_controller->downloadEnabled()
        && m_context.key.isValid()) {
        runSearch();
    }
}

void SubtitlesViewModel::setAttachOnDownload(bool on)
{
    if (m_attachOnDownload == on) {
        return;
    }
    m_attachOnDownload = on;
    Q_EMIT attachOnDownloadChanged();
}

void SubtitlesViewModel::setLanguages(const QStringList& codes)
{
    QStringList normalised;
    normalised.reserve(codes.size());
    for (const auto& c : codes) {
        const auto lo = c.trimmed().toLower();
        if (lo.size() == 3 && !normalised.contains(lo)) {
            normalised.append(lo);
        }
    }
    if (normalised == m_languages) {
        return;
    }
    m_languages = std::move(normalised);
    Q_EMIT filtersChanged();
}

void SubtitlesViewModel::setHi(const QString& mode)
{
    if (m_hi == mode) {
        return;
    }
    m_hi = mode;
    Q_EMIT filtersChanged();
}

void SubtitlesViewModel::setFpo(const QString& mode)
{
    if (m_fpo == mode) {
        return;
    }
    m_fpo = mode;
    Q_EMIT filtersChanged();
}

void SubtitlesViewModel::setRelease(const QString& release)
{
    if (m_release == release) {
        return;
    }
    m_release = release;
    Q_EMIT filtersChanged();
}

void SubtitlesViewModel::setSelectedRow(int row)
{
    if (m_selectedRow == row) {
        return;
    }
    m_selectedRow = row;
    Q_EMIT selectedRowChanged();
    updatePrimarySemantics();
}

void SubtitlesViewModel::runSearch()
{
    if (!m_controller || !m_context.key.isValid()) {
        return;
    }
    m_controller->runQuery(m_context.key, m_languages, m_hi, m_fpo,
        m_release.trimmed());
}

void SubtitlesViewModel::resetFilters()
{
    m_languages = m_settings.preferredLanguages();
    m_hi = m_settings.hearingImpaired();
    m_fpo = m_settings.foreignPartsOnly();
    m_release.clear();
    Q_EMIT filtersChanged();
}

void SubtitlesViewModel::runPrimaryAction()
{
    if (!m_controller || m_selectedRow < 0
        || m_selectedRow >= m_model->count()) {
        return;
    }
    const auto& hit = m_model->hitAt(m_selectedRow);
    if (hit.fileId.isEmpty()) {
        return;
    }
    m_controller->download(hit.fileId, m_context.key);
}

void SubtitlesViewModel::openLocalFile()
{
    // QML opens a `FileDialog` and calls back into
    // `localFileResolved(url)` with the chosen path. Splitting this
    // way keeps the VM headless.
}

void SubtitlesViewModel::localFileResolved(const QUrl& url)
{
    if (!url.isValid() || !m_context.key.isValid()) {
        return;
    }
    const auto path = url.isLocalFile() ? url.toLocalFile()
                                        : url.toString();
    if (path.isEmpty()) {
        return;
    }
    Q_EMIT localFileChosen(m_context.key, path);
}

void SubtitlesViewModel::openSettings()
{
    Q_EMIT settingsRequested();
}

void SubtitlesViewModel::onHitsChanged()
{
    if (!m_controller) {
        return;
    }
    m_model->setHits(m_controller->hits());
    refreshModelFlags();
    // Selection is meaningless across a fresh result set; reset
    // before signalling the new state so QML doesn't briefly show a
    // stale primary-action label.
    if (m_selectedRow != -1) {
        m_selectedRow = -1;
        Q_EMIT selectedRowChanged();
    }
    updateState();
    updateStatusLine();
    updatePrimarySemantics();
}

void SubtitlesViewModel::onCacheChanged()
{
    refreshModelFlags();
    updatePrimarySemantics();
}

void SubtitlesViewModel::onSearchingChanged()
{
    updateState();
    updateStatusLine();
    updatePrimarySemantics();
}

void SubtitlesViewModel::onErrorChanged()
{
    updateState();
    updateStatusLine();
    updatePrimarySemantics();
}

void SubtitlesViewModel::onDownloadEnabledChanged()
{
    updateState();
    updatePrimarySemantics();
}

void SubtitlesViewModel::onQuotaChanged()
{
    updateStatusLine();
}

void SubtitlesViewModel::onDownloadFinished(const QString& fileId,
    const QString& localPath, const QString& language,
    const QString& languageName)
{
    Q_EMIT downloadCompleted(m_context.key, fileId, localPath,
        language, languageName);
    // Bump cached/active flags so the row badge updates to "use" /
    // "re-attach" for the next selection.
    refreshModelFlags();
    updateStatusLine();
    updatePrimarySemantics();
}

void SubtitlesViewModel::onDownloadFailed(const QString&,
    const QString&)
{
    updateStatusLine();
    updatePrimarySemantics();
}

void SubtitlesViewModel::updateState()
{
    QString next;
    if (!m_controller) {
        next = QStringLiteral("notconfigured");
    } else if (!m_controller->downloadEnabled()) {
        next = QStringLiteral("notconfigured");
    } else if (m_controller->isSearching()) {
        next = QStringLiteral("loading");
    } else if (!m_controller->lastError().isEmpty()) {
        next = QStringLiteral("error");
    } else if (m_controller->hits().isEmpty()) {
        // "idle" if no search has run for this key yet (model
        // empty + controller has no hits). The two collapse to the
        // same UI: PlaceholderMessage with a "Pick a language and
        // search." copy. The plan distinguishes them mostly so the
        // empty-state copy can change post-search; we use the same
        // surface for both and let the search button trigger.
        next = m_model->count() == 0 && m_release.isEmpty()
            ? QStringLiteral("idle")
            : QStringLiteral("empty");
    } else {
        next = QStringLiteral("ready");
    }
    if (next != m_state) {
        m_state = next;
        Q_EMIT stateChanged();
    }
}

void SubtitlesViewModel::updateStatusLine()
{
    if (!m_controller) {
        if (!m_statusLine.isEmpty()) {
            m_statusLine.clear();
            Q_EMIT statusLineChanged();
        }
        return;
    }
    QStringList parts;
    if (m_controller->isSearching()) {
        parts << i18nc("@info:status subtitles search in progress",
            "Searching…");
    } else {
        const int n = m_controller->hits().size();
        parts << i18ncp("@info:status subtitles result count",
            "%1 result", "%1 results", n);
    }
    const int remaining = m_controller->dailyDownloadsRemaining();
    if (remaining >= 0) {
        parts << i18nc("@info:status OpenSubtitles daily download quota",
            "Daily quota: %1 remaining", remaining);
    }
    const auto next = parts.join(QStringLiteral("  ·  "));
    if (next != m_statusLine) {
        m_statusLine = next;
        Q_EMIT statusLineChanged();
    }
}

void SubtitlesViewModel::updatePrimarySemantics()
{
    QString text;
    QString icon;
    bool enabled = false;
    const bool gate = m_controller && m_controller->downloadEnabled()
        && m_context.key.isValid();

    if (!gate || m_selectedRow < 0
        || m_selectedRow >= m_model->count()) {
        text = i18nc("@action:button subtitle row primary action default label",
            "Download");
        icon = QStringLiteral("download");
    } else {
        const auto& hit = m_model->hitAt(m_selectedRow);
        const auto cached = m_controller->cachedFileIds();
        const auto active = m_controller->activeLocalPaths();
        const bool isCached = cached.contains(hit.fileId);
        const bool isActive = !hit.fileId.isEmpty() && [&] {
            // m_active stores local paths, not fileIds; an exact
            // match here is approximate (best-effort: whatever the
            // model thinks is active). Falling through to the
            // cached branch is acceptable.
            Q_UNUSED(active);
            return false;
        }();
        if (isActive) {
            text = i18nc("@action:button subtitle row action when already loaded "
                         "in the player",
                "Re-attach");
            icon = QStringLiteral("view-refresh");
        } else if (isCached) {
            text = i18nc("@action:button subtitle row action for cached entry",
                "Use");
            icon = QStringLiteral("dialog-ok-apply");
        } else {
            text = i18nc("@action:button subtitle row action for fresh download",
                "Download");
            icon = QStringLiteral("download");
        }
        enabled = true;
    }

    if (text == m_primaryActionText && icon == m_primaryActionIcon
        && enabled == m_primaryActionEnabled) {
        return;
    }
    m_primaryActionText = std::move(text);
    m_primaryActionIcon = std::move(icon);
    m_primaryActionEnabled = enabled;
    Q_EMIT primarySemanticsChanged();
}

void SubtitlesViewModel::refreshModelFlags()
{
    if (!m_controller || !m_model) {
        return;
    }
    m_model->setCachedFileIds(m_controller->cachedFileIds());
    // The widget version's `setActiveFileIds({})` is a placeholder
    // because the controller stores active subtitles by local path,
    // not fileId. Phase 06 carries that forward — the active flag
    // is best-effort and will be revisited if it surfaces a UX bug.
    m_model->setActiveFileIds({});
}

QString SubtitlesViewModel::formatContextTitle(
    const api::PlaybackContext& ctx) const
{
    QString base = ctx.title.isEmpty()
        ? i18nc("@title placeholder when media has no display title",
              "Selected media")
        : ctx.title;
    if (ctx.key.kind == api::MediaKind::Series
        && ctx.key.season.has_value()
        && ctx.key.episode.has_value()) {
        const QString tag = QStringLiteral("S%1E%2")
                                .arg(*ctx.key.season, 2, 10,
                                    QLatin1Char('0'))
                                .arg(*ctx.key.episode, 2, 10,
                                    QLatin1Char('0'));
        const auto series = ctx.seriesTitle.isEmpty() ? base
                                                       : ctx.seriesTitle;
        if (!ctx.episodeTitle.isEmpty()) {
            return i18nc("@info subtitles page header for an episode. "
                         "%1 series, %2 season+episode tag, %3 episode title",
                "%1 — %2 · %3", series, tag, ctx.episodeTitle);
        }
        return i18nc("@info subtitles page header for an episode without title",
            "%1 — %2", series, tag);
    }
    return base;
}

} // namespace kinema::ui::qml
